//! Shared orientation of an exhaustive binary, lazy CASE.
//!
//! This only chooses between `(condition, yes, no)` and its complemented,
//! swapped form. It neither evaluates an arm nor expands CASE into AND/OR.
use crate::{
    expr::{BinOpKind, ExprKind},
    sort_context::SortContext,
    symvar::SymVarOrigin,
    Expr, Lit,
};

fn is_integer_zero(value: &Expr) -> bool {
    value
        .as_lit()
        .and_then(Lit::as_integer)
        .is_some_and(|value| value == 0.into())
}

fn has_integer_evidence(value: &Expr) -> bool {
    match value.kind() {
        ExprKind::SymVar { origin } => match origin {
            SymVarOrigin::Parameter { primitive_type, .. }
            | SymVarOrigin::External { primitive_type, .. }
            | SymVarOrigin::Temporal { primitive_type, .. } => matches!(
                primitive_type.as_str(),
                "integer" | "i32" | "i64"
            ),
        },
        _ => false,
    }
}

fn is_positive_subject(value: &Expr) -> Option<&Expr> {
    let (op, left, right) = value.as_binop()?;
    match op {
        BinOpKind::Gt if is_integer_zero(right) => Some(left),
        BinOpKind::Lt if is_integer_zero(left) => Some(right),
        _ => None,
    }
}

fn is_already_positive_count_form(condition: &Expr, yes: &Expr, no: &Expr) -> bool {
    is_positive_subject(condition).is_some_and(|subject| {
        has_integer_evidence(subject) && is_integer_zero(no) && yes == subject
    })
}

/// Comparison with the THEN value on the left. Both arms must be exactly
/// the operands. This swaps comparison operands only and preserves ties.
pub fn selection_comparison(condition: &Expr, yes: &Expr, no: &Expr) -> Option<BinOpKind> {
    use BinOpKind::{Ge, Gt, Le, Lt};
    let (op, left, right) = condition.as_binop()?;
    if !matches!(op, Lt | Le | Gt | Ge) {
        return None;
    }
    if yes == left && no == right {
        return Some(op);
    }
    if yes != right || no != left {
        return None;
    }
    op.swapped_comparison()
}

/// For two values certified by the caller to have a total numeric order,
/// equality makes both arms indistinguishable. Choose one strict guard for
/// all four equivalent spellings of a minimum (or a maximum). A row, a
/// comparison token, or a default on an untyped SUM is no such certificate.
pub fn ordered_number_selection(
    condition: &Expr,
    yes: &Expr,
    no: &Expr,
    context: &dyn SortContext,
) -> Option<Expr> {
    let op = selection_comparison(condition, yes, no)?;
    // Keep literal defaults in ELSE, then minimize the cost of the value
    // repeated in WHEN and THEN. Stable semantic ordering breaks equal costs.
    let cost = |v: &Expr| (v.as_lit().is_some(), v.size());
    let order = cost(yes)
        .cmp(&cost(no))
        .then_with(|| crate::canonical_order_compare(yes, no, context));
    let (first, second) = if order.is_gt() { (no, yes) } else { (yes, no) };
    let cmp = if matches!(op, BinOpKind::Le | BinOpKind::Lt) {
        crate::builder::lt
    } else {
        crate::builder::gt
    };
    let guard = crate::normalize_with(cmp(first.clone(), second.clone()), context);
    Some(crate::builder::case_when(vec![
        (guard, first.clone()),
        (crate::builder::lit_bool(true), second.clone()),
    ]))
}

fn explicit_integer(value: &Expr) -> bool {
    has_integer_evidence(value) || matches!(value.as_lit(), Some(Lit::Int(_) | Lit::BigInt(_)))
}

// Lower ranks are the published semantic policies.  The policy is part of
// the key before the structural tie-breakers, so a generic comparator cannot
// undo a domain-specific representative on a later normalization pass.
fn domain_policy_rank(condition: &Expr, yes: &Expr, no: &Expr) -> u8 {
    // A numeric indicator names its true domain and falls back to zero.
    let indicator = is_integer_zero(no)
        && yes
            .as_lit()
            .and_then(Lit::as_integer)
            .is_some_and(|n| n == 1.into());
    if indicator || is_already_positive_count_form(condition, yes, no) {
        return 0;
    }
    if no.is_null() {
        return 1;
    }
    2
}

fn candidate_precedes(
    left_condition: &Expr,
    left_yes: &Expr,
    left_no: &Expr,
    right_condition: &Expr,
    right_yes: &Expr,
    right_no: &Expr,
    context: &dyn SortContext,
) -> bool {
    let left_policy = domain_policy_rank(left_condition, left_yes, left_no);
    let right_policy = domain_policy_rank(right_condition, right_yes, right_no);
    if left_policy != right_policy {
        return left_policy < right_policy;
    }
    crate::canonical_order_compare(left_condition, right_condition, context)
        .then_with(|| crate::canonical_order_compare(left_yes, right_yes, context))
        .then_with(|| crate::canonical_order_compare(left_no, right_no, context))
        .is_lt()
}

/// Return the preferred complement if the existing binary CASE direction
/// prefers swapping both arms; otherwise return None.
///
/// The caller must retain exhaustive first-match CASE semantics and establish
/// that the guard is available at its evaluation site. Arm values may remain
/// partial outside their respective branches. Nested CASE arms retain their
/// current orientation, matching the ordinary CASE simplifier.
pub fn preferred_complement(
    condition: &Expr,
    yes: &Expr,
    no: &Expr,
    context: &dyn SortContext,
) -> Option<Expr> {
    if yes.as_case_when().is_some() || no.as_case_when().is_some() {
        return None;
    }
    let complement = crate::normalize_with(crate::builder::not(condition.clone()), context);
    // Complexity is a candidate constraint, never a sorting preference.
    let negate = |left: &Expr, right: &Expr| {
        matches!(left.as_unary_op(), Some((crate::expr::UnaryOpKind::Not, operand)) if operand == right)
    };
    // A lone NOT only reverses the CASE decision; it does not add a
    // second predicate evaluation or receipt observation.
    if complement.size() != condition.size()
        && !negate(&complement, condition) && !negate(condition, &complement)
    {
        return None;
    }
    prefer_complement(condition, &complement, yes, no, context).then_some(complement)
}

/// Compare an already canonical complement supplied by a certified local
/// domain. The caller proves complementary guards in that SAME domain; this
/// function only reuses the common structural direction, never normalizes CASE.
fn prefer_complement(
    condition: &Expr,
    complement: &Expr,
    yes: &Expr,
    no: &Expr,
    context: &dyn SortContext,
) -> bool {
    // Selecting one of the compared values has its own canonicalization
    // below. Preserve an untyped choice rather than assuming a total order.
    if selection_comparison(condition, yes, no).is_some() {
        return false;
    }
    candidate_precedes(complement, no, yes, condition, yes, no, context)
}

/// Return the canonical representative when a total binary CASE should use
/// its complemented guard.  This is the single structural entry point for
/// callers that must preserve CASE laziness while sharing the published
/// domain policy and ordering key.
pub fn canonicalize_binary_case(expression: &Expr, context: &dyn SortContext) -> Option<Expr> {
    let branches = expression.as_case_when()?;
    if branches.len() != 2
        || branches[1].0.as_bool() != Some(true)
        || branches
            .iter()
            .any(|(_, value)| value.as_case_when().is_some())
    {
        return None;
    }
    let (condition, yes) = &branches[0];
    let no = &branches[1].1;
    if selection_comparison(condition, yes, no).is_some() {
        if !explicit_integer(yes) || !explicit_integer(no) {
            return None;
        }
        let canonical = ordered_number_selection(condition, yes, no, context)?;
        return (canonical != *expression).then_some(canonical);
    }
    let complement = preferred_complement(&branches[0].0, &branches[0].1, &branches[1].1, context)?;
    Some(crate::builder::case_when(vec![
        (complement, branches[1].1.clone()),
        (crate::builder::lit_bool(true), branches[0].1.clone()),
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{builder as b, DefaultSortContext};

    #[test]
    fn structural_case_key_uses_both_arms_to_break_equal_guards() {
        let guard = b::param("guard", "boolean");
        let one = b::lit_int(1);
        let seven = b::lit_int(7);
        let nine = b::lit_int(9);
        assert!(candidate_precedes(&guard, &one, &nine, &guard, &seven, &nine, &DefaultSortContext));
        assert!(candidate_precedes(&guard, &one, &seven, &guard, &one, &nine, &DefaultSortContext));
        assert!(!candidate_precedes(&guard, &one, &nine, &guard, &one, &seven, &DefaultSortContext));
    }
}
