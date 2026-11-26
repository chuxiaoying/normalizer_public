//! 关系与区间蕴含证明规则。

use super::*;

pub(super) fn conjunction_implies(conjs_left: &[Expr], conjs_right: &[Expr]) -> bool {
    conjs_right.iter().all(|right| expr_is_true_under_and_context(right, conjs_left))
}

// Callers already exclude unrelated pairs before applying OR proof rules.
pub(super) fn or_implication_tautology(left: &Expr, right: &Expr) -> bool {
    if relation_cardinality_exhaustive(left, right) {
        return true;
    }

    // If A => B is a proven relation/cardinality fact, then (!A || B) is true.
    // Keep this OR-only: the dual AND contradiction is directional (A && !B),
    // so this must not be folded into the symmetric `is_complement` helper.
    if let ExprKind::UnaryOp {
        op: UnaryOpKind::Not,
        operand,
    } = left.kind()
    {
        if relation_count_nonzero_implies_exists(operand, right) {
            return true;
        }
    }
    if let ExprKind::UnaryOp {
        op: UnaryOpKind::Not,
        operand,
    } = right.kind()
    {
        if relation_count_nonzero_implies_exists(operand, left) {
            return true;
        }
    }
    false
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

pub(super) fn relation_cardinality_exhaustive(left: &Expr, right: &Expr) -> bool {
    relation_count_nonzero_or_zero_same_relation(left, right)
        || relation_count_nonzero_or_zero_same_relation(right, left)
        || exists_and_count_zero_same_relation(left, right)
        || exists_and_count_zero_same_relation(right, left)
        || relation_exists_or_not_exists_same_relation(left, right)
        || relation_exists_or_not_exists_same_relation(right, left)
}

pub(super) fn relation_count_nonzero_or_zero_same_relation(
    nonzero_expr: &Expr,
    zero_expr: &Expr,
) -> bool {
    let Some(nonzero_relation) = relation_count_nonzero_relation(nonzero_expr) else {
        return false;
    };
    let Some(zero_relation) = relation_count_zero_relation(zero_expr) else {
        return false;
    };
    relation_cardinality_input_matches(nonzero_relation, zero_relation)
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
    let Some((UnaryOpKind::Not, not_exists_inner)) = not_exists_expr.as_unary_op() else {
        return false;
    };
    let Some(not_exists_relation) = relation_exists_true_relation(not_exists_inner) else {
        return false;
    };
    relation_cardinality_input_matches(exists_relation, not_exists_relation)
}

pub(super) fn relation_count_nonzero_implies_exists(left: &Expr, right: &Expr) -> bool {
    let Some(left_relation) =
        relation_count_nonzero_relation(left).or_else(|| relation_count_positive_lower_bound(left))
    else {
        return false;
    };
    let Some(right_relation) = relation_exists_true_relation(right) else {
        return false;
    };
    relation_cardinality_input_matches(left_relation, right_relation)
}

/// Emptiness of an identical input implies emptiness of any strict chain
/// of selections over that input. Do not infer row identity from aliases.
pub(super) fn empty_input_implies_empty_selection(left: &Expr, right: &Expr) -> bool {
    use crate::domains::relation::RelationExpr;
    fn absent(expr: &Expr) -> Option<&RelationExpr> {
        let (UnaryOpKind::Not, exists) = expr.as_unary_op()? else { return None; };
        relation_exists_true_relation(exists)
    }
    let (Some(source), Some(mut candidate)) = (absent(left), absent(right)) else { return false; };
    while let RelationExpr::Select { input, .. } = candidate {
        if input.as_ref() == source { return true; }
        candidate = input;
    }
    false
}

/// A positive exact cardinality bound implies existence, but existence does
/// NOT imply that stronger bound. Keep this separate from the bidirectional
/// nonzero matcher used by the reverse implication and zero contradictions.
pub(super) fn relation_count_positive_lower_bound(
    expr: &Expr,
) -> Option<&crate::domains::relation::RelationExpr> {
    let (mut op, left, right) = expr.as_binop()?;
    let (query, n) = if let Some(crate::Lit::Int(n)) = right.as_lit() {
        (left, *n)
    } else if let Some(crate::Lit::Int(n)) = left.as_lit() {
        op = op.swapped_comparison()?;
        (right, *n)
    } else {
        return None;
    };
    let proves_positive = match op {
        BinOpKind::Gt => n >= 0,
        BinOpKind::Ge | BinOpKind::Eq => n >= 1,
        _ => false,
    };
    if !proves_positive {
        return None;
    }
    let (RelationAggregateKind::Count, relation, None, None) = query.as_relation_aggregate()?
    else {
        return None;
    };
    Some(relation)
}

pub(super) fn relation_cardinality_input_matches(
    left: &crate::domains::relation::RelationExpr,
    right: &crate::domains::relation::RelationExpr,
) -> bool {
    if left == right {
        return true;
    }
    matches!(
        (left, right),
        (
            crate::domains::relation::RelationExpr::Base { name: left_name, .. },
            crate::domains::relation::RelationExpr::Base { name: right_name, .. }
        ) if left_name == right_name
    )
}

pub(super) fn relation_exists_true_relation(
    expr: &Expr,
) -> Option<&crate::domains::relation::RelationExpr> {
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

pub(super) fn relation_count_nonzero_relation(
    expr: &Expr,
) -> Option<&crate::domains::relation::RelationExpr> {
    let ExprKind::BinOp { op, left, right } = expr.kind() else {
        return None;
    };

    match op {
        BinOpKind::Ne => relation_count_zero_comparison(left, right)
            .or_else(|| relation_count_zero_comparison(right, left)),
        BinOpKind::Gt => relation_count_zero_comparison(left, right),
        BinOpKind::Lt => relation_count_zero_comparison(right, left),
        _ => None,
    }
}

pub(super) fn relation_count_zero_relation(
    expr: &Expr,
) -> Option<&crate::domains::relation::RelationExpr> {
    if let ExprKind::UnaryOp {
        op: UnaryOpKind::Not,
        operand,
    } = expr.kind()
    {
        return relation_count_nonzero_relation(operand);
    }

    let ExprKind::BinOp { op, left, right } = expr.kind() else {
        return None;
    };
    if *op != BinOpKind::Eq {
        return None;
    }
    relation_count_zero_comparison(left, right)
        .or_else(|| relation_count_zero_comparison(right, left))
}

pub(super) fn relation_count_zero_comparison<'a>(
    maybe_count: &'a Expr,
    maybe_zero: &Expr,
) -> Option<&'a crate::domains::relation::RelationExpr> {
    if maybe_zero.as_lit() != Some(&Lit::Int(0)) {
        return None;
    }
    let Some((RelationAggregateKind::Count, relation, select_expr, default_expr)) =
        maybe_count.as_relation_aggregate()
    else {
        return None;
    };
    if select_expr.is_some() || default_expr.is_some() {
        return None;
    }
    Some(relation)
}
