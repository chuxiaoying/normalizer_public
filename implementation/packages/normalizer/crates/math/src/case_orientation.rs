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

fn direct_exact_number(value: &Expr) -> bool {
    matches!(value.as_sym_var(), Some(SymVarOrigin::Parameter { name, primitive_type })
        if matches!(primitive_type.as_str(), "integer" | crate::domains::decimal::VALUE_TAG)
            && !name.starts_with('$') && !name.contains(['.', '[', ']']))
}

/// Equality of already-acquired input numbers can establish an effective
/// arm domain without admitting arbitrary observations as proof atoms.
pub(crate) fn is_exact_input_equality(expression: &Expr) -> bool {
    matches!(expression.as_binop(), Some((BinOpKind::Eq, left, right))
        if direct_exact_number(left) && direct_exact_number(right))
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
    order_selection(op, yes, no, context)
}

fn order_selection(op: BinOpKind, yes: &Expr, no: &Expr, context: &dyn SortContext) -> Option<Expr> {
    if !crate::canonical_order::comparison_operands_are_bound(yes, context)
        || !crate::canonical_order::comparison_operands_are_bound(no, context)
    {
        return None;
    }
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
    Some(crate::builder::case_when(vec![(guard, when_greater.clone())], otherwise.clone()))
}

fn has_total_number_selection_evidence(value: &Expr, context: &dyn SortContext) -> bool {
    // A native comparison of the two exact, bound operands already
    // acquires both values before selecting either. Arithmetic over those
    // values contributes no new binding or observation to the selection.
    if (value.as_call().is_some() || matches!(value.as_relation_aggregate(),
        Some((crate::RelationAggregateKind::Count | crate::RelationAggregateKind::Sum, ..))))
        && crate::normalize::exact_acquired_arithmetic(value, context)
    { return true; }
    if let Some(SymVarOrigin::Temporal { binding_id, primitive_type }) = value.as_sym_var() {
        return primitive_type == "integer" && (context.temporal_is_total(*binding_id)
            || context.choice_slots().is_some_and(|slots| slots.contains_key(binding_id)));
    }
    if let Some(SymVarOrigin::External { binding_id, path, primitive_type }) = value.as_sym_var() {
        return path.is_empty() && matches!(primitive_type.as_str(), "integer" | crate::domains::decimal::VALUE_TAG)
            && context.observation_identities().and_then(|identities| identities.get(binding_id))
                .is_some_and(|identity| matches!(identity, crate::ObservationIdentity::EntrySnapshot(_)));
    }
    direct_exact_number(value) || matches!(value.as_lit(), Some(Lit::Int(_) | Lit::BigInt(_)))
}

/// A final min/max is a complete two-value decision in the remaining
/// domain. Its already acquired total operands may use CF-04 without
/// moving any earlier arm or treating arbitrary suffix guards as binary.
pub(crate) fn canonicalize_terminal_extremum(expression: &Expr, context: &dyn SortContext) -> Option<Expr> {
    let (arms, fallback) = expression.as_total_case_when()?;
    let ((condition, selected), earlier) = arms.split_last()?;
    if earlier.is_empty() || !has_total_number_selection_evidence(selected, context)
        || !has_total_number_selection_evidence(fallback, context)
    { return None; }
    let op = selection_comparison(condition, selected, fallback)?;
    let suffix = order_selection(op, selected, fallback, context)?;
    let ([(condition, selected)], fallback) = suffix.as_total_case_when()? else { return None; };
    let mut rewritten = earlier.to_vec();
    rewritten.push((condition.clone(), selected.clone()));
    let result = crate::builder::case_when(rewritten, fallback.clone());
    (result != *expression).then_some(result)
}

/// A final receipt decision is binary in its remaining domain. The same
/// closed-completion rule applies without moving any earlier CASE arm.
pub(crate) fn canonicalize_terminal_completion(expression: &Expr, context: &dyn SortContext) -> Option<Expr> {
    let (arms, fallback) = expression.as_total_case_when()?;
    let ((condition, result), earlier) = arms.split_last()?;
    if earlier.is_empty() { return None; }
    let (receipt, classes) = crate::domains::completion::failure_partition(condition.as_not().unwrap_or(condition))?;
    let Some(SymVarOrigin::External { binding_id, .. }) = receipt.as_sym_var() else { return None; };
    if context.total_failure_classes(*binding_id).as_deref() != Some(classes.as_slice()) { return None; }
    let suffix = crate::builder::case_when(vec![(condition.clone(), result.clone())], fallback.clone());
    let selected = canonicalize_binary_case(&suffix, context, None)?;
    let ([(condition, result)], fallback) = selected.as_total_case_when()? else { return None; };
    let mut arms = earlier.to_vec();
    arms.push((condition.clone(), result.clone()));
    let new = crate::builder::case_when(arms, fallback.clone());
    (new != *expression).then_some(new)
}

fn preferred_complement_for_case(
    condition: &Expr, yes: &Expr, no: &Expr, context: &dyn SortContext,
) -> Option<Expr> {
    // In SQL a third outcome can fall through in both orientations.
    // NOT UNKNOWN does not select the former ELSE arm.
    if condition.may_observe_sql_unknown() { return None; }
    let complement = crate::normalize_with(crate::builder::not(condition.clone()), context);
    let original = crate::builder::case_when(vec![(condition.clone(), yes.clone())], no.clone());
    let swapped = crate::builder::case_when(vec![(complement.clone(), no.clone())], yes.clone());
    crate::canonical_order_compare(&swapped, &original, context).is_lt().then_some(complement)
}

/// Return the canonical representative when a total binary CASE should use
/// its complemented guard.  This is the single structural entry point for
/// callers that must preserve CASE laziness while sharing the published
/// predicate polarity and ordering key.
pub(crate) fn canonicalize_binary_case(
    expression: &Expr, context: &dyn SortContext, parent: Option<&Expr>,
) -> Option<Expr> {
    let ([(condition, yes)], no) = expression.as_total_case_when()? else {
        return None;
    };
    use crate::domains::completion;
    // Compare both polarities of an acquired receipt only if the observed
    // tag set is its entire closed failure partition, not a catch subset.
    let failed = condition.as_not().unwrap_or(condition);
    if let Some((receipt, classes)) = completion::failure_partition(failed) {
        if let Some(SymVarOrigin::External { binding_id, .. }) = receipt.as_sym_var() {
            if context.total_failure_classes(*binding_id).as_deref() == Some(classes.as_slice()) {
                let (success, failure) = if condition.as_not().is_some() { (yes, no) } else { (no, yes) };
                let candidate = crate::builder::case_when(
                    vec![(completion::closed_return(receipt.clone(), &classes), success.clone())],
                    failure.clone(),
                );
                if crate::canonical_order_compare(&candidate, expression, context).is_lt() {
                    return Some(candidate);
                }
            }
        }
    }
    if yes.as_case_when().is_some() || no.as_case_when().is_some() {
        return None;
    }
    if let Some(op) = selection_comparison(condition, yes, no) {
        if !has_total_number_selection_evidence(yes, context) || !has_total_number_selection_evidence(no, context) {
            return None;
        }
        // Under a proven reachable equality domain, the two equivalent
        // extreme-value spellings collapse to different acquired operands.
        // Run the candidate selection before ordinary guard simplification.
        // A single equality of two direct exact inputs is reachable; don't
        // infer reachability from a general implication on an unknown domain.
        if direct_exact_number(yes) && direct_exact_number(no)
            && parent.and_then(Expr::as_binop).is_some_and(|(op, left, right)|
                op == BinOpKind::Eq && ((left == yes && right == no) || (left == no && right == yes)))
        {
            let selected = if crate::canonical_order_compare(yes, no, context).is_gt() { no } else { yes };
            return Some(selected.clone());
        }
        let canonical = order_selection(op, yes, no, context)?;
        return (canonical != *expression).then_some(canonical);
    }
    let literal_point = |value: &Expr| value.as_lit()?.as_integer();
    if let Some((source, literal, when_true_is_source)) = match (literal_point(yes), literal_point(no)) {
        (None, Some(point)) => Some((yes, (no, point), true)),
        (Some(point), None) => Some((no, (yes, point), false)),
        _ => None,
    } {
        let (literal, point) = literal;
        if let Some(op) = crate::normalize::integer_ranges::integer_selection(
            condition, source, &point, when_true_is_source, parent,
        ) {
            let candidate = order_selection(op, source, literal, context)?;
            return (candidate != *expression).then_some(candidate);
        }
    }
    let complement = preferred_complement_for_case(condition, yes, no, context)?;
    Some(crate::builder::case_when(vec![(complement, no.clone())], yes.clone()))
}
