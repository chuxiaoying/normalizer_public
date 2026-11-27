//! Shared orientation of an exhaustive binary, lazy CASE.
//!
//! This only chooses between `(condition, yes, no)` and its complemented,
//! swapped form. It neither evaluates an arm nor expands CASE into AND/OR.
use crate::{
    expr::BinOpKind,
    sort_context::SortContext,
    symvar::SymVarOrigin,
    Expr, Lit,
};

fn has_integer_evidence(value: &Expr) -> bool {
    value.as_sym_var().is_some_and(|origin| origin.primitive_type() == "integer")
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
    Some(order_selection(op, yes, no, context))
}

fn order_selection(op: BinOpKind, yes: &Expr, no: &Expr, context: &dyn SortContext) -> Expr {
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
    crate::builder::case_when(vec![(guard, first.clone())], second.clone())
}

fn has_total_number_selection_evidence(value: &Expr) -> bool {
    has_integer_evidence(value)
        || matches!(value.as_lit(), Some(Lit::Int(_) | Lit::BigInt(_)))
        // The adapter tags finite exact decimal inputs explicitly. Only a
        // direct parameter is acquired unconditionally; a numeric field or
        // receipt annotation alone would not prove its evaluation domain.
        || matches!(
            value.as_sym_var(),
            Some(SymVarOrigin::Parameter { name, primitive_type })
                if primitive_type == crate::domains::decimal::VALUE_TAG
                    && !name.starts_with('$')
                    && !name.contains(['.', '[', ']'])
        )
}

// Complements are already proved by the caller and normalized in the
// same context. Choose the base of the predicate pair without inspecting
// the arm values: an ELSE null or a numeric 0/1 is not a direction policy.
fn compare_polarity(left: &Expr, right: &Expr) -> Option<std::cmp::Ordering> {
    use crate::expr::UnaryOpKind::Not;
    use std::cmp::Ordering::{Greater, Less};
    if matches!(right.as_unary_op(), Some((Not, operand)) if operand == left) {
        return Some(Less);
    }
    if matches!(left.as_unary_op(), Some((Not, operand)) if operand == right) {
        return Some(Greater);
    }
    let (l, a, b) = left.as_binop()?;
    let (r, c, d) = right.as_binop()?;
    if a != c || b != d || l.complemented_comparison() != Some(r) {
        return None;
    }
    match l {
        BinOpKind::Gt | BinOpKind::Lt | BinOpKind::Eq => Some(Less),
        BinOpKind::Le | BinOpKind::Ge | BinOpKind::Ne => Some(Greater),
        _ => None,
    }
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
    if let Some(order) = compare_polarity(left_condition, right_condition) {
        return order.is_lt();
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
    preferred_complement_for_shape(condition, yes, no, context, || {
        selection_comparison(condition, yes, no).is_some()
    })
}

fn preferred_complement_for_shape(
    condition: &Expr,
    yes: &Expr,
    no: &Expr,
    context: &dyn SortContext,
    is_selection: impl FnOnce() -> bool,
) -> Option<Expr> {
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
    // A typed selection has a separate direction. Keep complement
    // normalization before this rejection, as in the signed host path.
    (!is_selection() && candidate_precedes(&complement, no, yes, condition, yes, no, context))
        .then_some(complement)
}

/// Return the canonical representative when a total binary CASE should use
/// its complemented guard.  This is the single structural entry point for
/// callers that must preserve CASE laziness while sharing the published
/// predicate polarity and ordering key.
pub fn canonicalize_binary_case(expression: &Expr, context: &dyn SortContext) -> Option<Expr> {
    let ([(condition, yes)], no) = expression.as_total_case_when()? else {
        return None;
    };
    if yes.as_case_when().is_some() || no.as_case_when().is_some() {
        return None;
    }
    if let Some(op) = selection_comparison(condition, yes, no) {
        if !has_total_number_selection_evidence(yes) || !has_total_number_selection_evidence(no) {
            return None;
        }
        let canonical = order_selection(op, yes, no, context);
        return (canonical != *expression).then_some(canonical);
    }
    let complement = preferred_complement_for_shape(condition, yes, no, context, || false)?;
    Some(crate::builder::case_when(vec![(complement, no.clone())], yes.clone()))
}
