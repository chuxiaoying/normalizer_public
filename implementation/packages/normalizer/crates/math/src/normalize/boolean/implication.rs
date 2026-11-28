//! 关系与区间蕴含证明规则。

use super::*;
use crate::domains::relation::RelationExpr;
use crate::normalize::relation::relation_count_bound;

pub(super) fn conjunction_implies(conjs_left: &[Expr], conjs_right: &[Expr]) -> bool {
    conjs_right.iter().all(|right| expr_is_true_under_and_context(right, conjs_left))
}

pub(super) fn profile_pair_can_skip_proof_check(left: &Expr, right: &Expr) -> bool {
    if left == right || left.as_bool().is_some() || right.as_bool().is_some() {
        return false;
    }
    if left.vars().is_empty() || right.vars().is_empty() {
        return false;
    }
    left.vars().is_disjoint(right.vars()) && !(left.has_heap_ref() && right.has_heap_ref())
}

pub(super) fn relation_count_nonzero_implies_exists(left: &Expr, right: &Expr) -> bool {
    let Some((op, left_relation, bound)) = relation_count_bound(left) else {
        return false;
    };
    if op != BinOpKind::Gt || bound < 0.into() {
        return false;
    }
    let Some(right_relation) = relation_exists_true_relation(right) else {
        return false;
    };
    left_relation == right_relation
}

/// The empty member of an exact cardinality interval is the length-zero
/// observation of the same acquired relation, not an independent failure.
pub(super) fn empty_relation_is_within_count(empty: &Expr, count: &Expr) -> bool {
    let Some(exists) = empty.as_not() else { return false; };
    let (Some(empty), Some((op, relation, bound))) =
        (relation_exists_true_relation(exists), relation_count_bound(count))
    else { return false; };
    empty == relation && match op {
        BinOpKind::Lt => bound > 0.into(),
        BinOpKind::Le => bound >= 0.into(),
        _ => false,
    }
}

/// A selection of an empty input is empty.
pub(super) fn empty_input_implies_empty_selection(left: &Expr, right: &Expr) -> bool {
    fn empty_relation(expr: &Expr) -> Option<&RelationExpr> {
        let exists = expr.as_not()?;
        let (RelationAggregateKind::Exists, input, Some(value), None) =
            exists.as_relation_aggregate()? else { return None };
        (value.as_bool() == Some(true)).then_some(input)
    }

    let (Some(source), Some(candidate)) = (empty_relation(left), empty_relation(right))
    else { return false };
    matches!(candidate, RelationExpr::Select { input, .. } if input.as_ref() == source)
}

pub(super) fn relation_exists_true_relation(
    expr: &Expr,
) -> Option<&RelationExpr> {
    let (RelationAggregateKind::Exists, relation, Some(value), None) =
        expr.as_relation_aggregate()? else { return None; };
    (value.as_bool() == Some(true)).then_some(relation)
}
