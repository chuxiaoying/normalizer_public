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
fn selection_comparison(condition: &Expr, yes: &Expr, no: &Expr) -> Option<BinOpKind> {
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
    // CF-04: fix the operand order before generating strict comparison
    // candidates. Both have the same slots and cost, so > wins over <.
    let (left, right) = if crate::canonical_order_compare(yes, no, context).is_gt() {
        (no, yes)
    } else {
        (yes, no)
    };
    let guard = crate::normalize_with(crate::builder::gt(left.clone(), right.clone()), context);
    let (when_greater, otherwise) = if matches!(op, BinOpKind::Le | BinOpKind::Lt) {
        (right, left)
    } else {
        (left, right)
    };
    crate::builder::case_when(vec![(guard, when_greater.clone())], otherwise.clone())
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

/// Compare the full trees after normalizing the complement. Terminal selection
/// is still lazy: exchanging the two arms does not evaluate either arm in the
/// other arm's domain. Callers must first establish a total Boolean test in
/// the domain where it is already evaluated.
pub fn preferred_complement(
    condition: &Expr,
    yes: &Expr,
    no: &Expr,
    context: &dyn SortContext,
) -> Option<Expr> {
    if selection_comparison(condition, yes, no).is_some() { return None; }
    preferred_complement_for_case(condition, yes, no, context)
}

fn preferred_complement_for_case(
    condition: &Expr, yes: &Expr, no: &Expr, context: &dyn SortContext,
) -> Option<Expr> {
    let complement = crate::normalize_with(crate::builder::not(condition.clone()), context);
    let original = crate::builder::case_when(vec![(condition.clone(), yes.clone())], no.clone());
    let swapped = crate::builder::case_when(vec![(complement.clone(), no.clone())], yes.clone());
    crate::canonical_order_compare(&swapped, &original, context).is_lt().then_some(complement)
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
    let complement = preferred_complement_for_case(condition, yes, no, context)?;
    Some(crate::builder::case_when(vec![(complement, no.clone())], yes.clone()))
}
