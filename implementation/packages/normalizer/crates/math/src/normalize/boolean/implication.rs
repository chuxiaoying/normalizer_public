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
    exists_and_count_zero_same_relation(left, right)
        || exists_and_count_zero_same_relation(right, left)
}

fn exists_and_count_zero_same_relation(
    exists_expr: &Expr,
    zero_expr: &Expr,
) -> bool {
    let Some(exists_relation) = relation_exists_true_relation(exists_expr) else {
        return false;
    };
    let Some(zero_relation) = relation_count_zero_relation(zero_expr) else {
        return false;
    };
    relation_cardinality_input_matches(exists_relation, zero_relation)
}

pub(super) fn relation_exists_or_not_exists_same_relation(
    exists_expr: &Expr,
    not_exists_expr: &Expr,
) -> bool {
    let Some(exists_relation) = relation_exists_true_relation(exists_expr) else {
        return false;
    };
    let Some(not_exists_relation) = relation_not_exists_true_relation(not_exists_expr) else {
        return false;
    };
    relation_cardinality_input_matches(exists_relation, not_exists_relation)
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
    let (Some(source), Some(mut candidate)) = (
        relation_not_exists_true_relation(left),
        relation_not_exists_true_relation(right),
    ) else {
        return false;
    };
    while let RelationExpr::Select { input, .. } = candidate {
        if input.as_ref() == source { return true; }
        candidate = input;
    }
    false
}

pub(super) fn relation_cardinality_input_matches(
    left: &RelationExpr,
    right: &RelationExpr,
) -> bool {
    if left == right {
        return true;
    }
    matches!(
        (left, right),
        (
            RelationExpr::Base { name: left_name, .. },
            RelationExpr::Base { name: right_name, .. }
        ) if left_name == right_name
    )
}

pub(super) fn relation_exists_true_relation(
    expr: &Expr,
) -> Option<&RelationExpr> {
    let Some((RelationAggregateKind::Exists, relation, select_expr, default_expr)) =
        expr.as_relation_aggregate()
    else {
        return None;
    };
    if select_expr.and_then(|expr| expr.as_bool()) != Some(true) || default_expr.is_some() {
        return None;
    }
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
