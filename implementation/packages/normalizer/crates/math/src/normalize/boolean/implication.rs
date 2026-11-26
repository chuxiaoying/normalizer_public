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

pub(super) fn relation_cardinality_contradicts(left: &Expr, right: &Expr) -> bool {
    exists_with_opposite(left, right, relation_count_zero_relation)
}

pub(super) fn relation_exists_complements(left: &Expr, right: &Expr) -> bool {
    exists_with_opposite(left, right, relation_not_exists_true_relation)
}

fn exists_with_opposite(
    left: &Expr, right: &Expr, opposite: fn(&Expr) -> Option<&RelationExpr>,
) -> bool {
    // Neither a count comparison nor a negated Exists is itself a positive
    // Exists. If both sides are positive, neither orientation can succeed.
    let (exists_relation, other) = if let Some(relation) = relation_exists_true_relation(left) {
        (relation, right)
    } else if let Some(relation) = relation_exists_true_relation(right) {
        (relation, left)
    } else {
        return false;
    };
    opposite(other).is_some_and(|relation| relation_cardinality_input_matches(exists_relation, relation))
}

pub(super) fn relation_count_nonzero_implies_exists(left: &Expr, right: &Expr) -> bool {
    let Some((op, left_relation, bound)) = relation_count_bound(left) else {
        return false;
    };
    // Stronger bounds imply Exists, not the reverse. The zero complement
    // uses only the exact equivalences below.
    if !match op {
        BinOpKind::Ne => bound == 0,
        BinOpKind::Gt => bound >= 0,
        BinOpKind::Ge | BinOpKind::Eq => bound >= 1,
        _ => false,
    } {
        return false;
    }
    let Some(right_relation) = relation_exists_true_relation(right) else {
        return false;
    };
    relation_cardinality_input_matches(left_relation, right_relation)
}

/// Emptiness of an identical input implies emptiness of any strict chain
/// of selections over that input. Do not infer row identity from aliases.
pub(super) fn empty_input_implies_empty_selection(left: &Expr, right: &Expr) -> bool {
    false
}

pub(super) fn relation_cardinality_input_matches(
    left: &RelationExpr,
    right: &RelationExpr,
) -> bool {
    left == right
}

pub(super) fn relation_exists_true_relation(
    expr: &Expr,
) -> Option<&RelationExpr> {
    let Some((RelationAggregateKind::Exists, relation, select_expr, default_expr)) =
        expr.as_relation_aggregate()
    else {
        return None;
    };
    Some(relation)
}

fn relation_not_exists_true_relation(expr: &Expr) -> Option<&RelationExpr> {
    let (UnaryOpKind::Not, exists) = expr.as_unary_op()? else {
        return None;
    };
    relation_exists_true_relation(exists)
}

pub(super) fn relation_count_zero_relation(expr: &Expr) -> Option<&RelationExpr> {
    let (expr, negated) = match expr.as_unary_op() {
        Some((UnaryOpKind::Not, operand)) => (operand, true),
        _ => (expr, false),
    };
    let (op, relation, 0) = relation_count_bound(expr)? else { return None; };
    matches!((op, negated), (BinOpKind::Eq, false) | (BinOpKind::Ne | BinOpKind::Gt, true))
        .then_some(relation)
}
