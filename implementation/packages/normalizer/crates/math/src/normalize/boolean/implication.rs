//! 关系与区间蕴含证明规则。

use super::*;

pub(super) fn conjunction_implies(conjs_left: &[Expr], conjs_right: &[Expr]) -> bool {
    conjs_right.iter().all(|right| {
        expr_is_true_under_and_context(right, conjs_left)
            || conjs_left
                .iter()
                .any(|left| relation_exists_conjunct_implies(left, right))
    })
}

pub(super) fn or_implication_tautology(left: &Expr, right: &Expr) -> bool {
    if profile_pair_can_skip_proof_check(left, right) {
        return false;
    }
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
        if relation_exists_conjunct_implies(operand, right) {
            return true;
        }
    }
    if let ExprKind::UnaryOp {
        op: UnaryOpKind::Not,
        operand,
    } = right.kind()
    {
        if relation_exists_conjunct_implies(operand, left) {
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

pub(super) fn relation_exists_conjunct_implies(left: &Expr, right: &Expr) -> bool {
    if profile_pair_can_skip_proof_check(left, right) {
        return false;
    }
    relation_count_nonzero_implies_exists(left, right)
}

pub(super) fn relation_cardinality_contradicts(left: &Expr, right: &Expr) -> bool {
    relation_exists_contradicts_count_zero(left, right)
        || relation_exists_contradicts_count_zero(right, left)
}

pub(super) fn relation_cardinality_exhaustive(left: &Expr, right: &Expr) -> bool {
    relation_count_nonzero_or_zero_same_relation(left, right)
        || relation_count_nonzero_or_zero_same_relation(right, left)
        || relation_exists_or_count_zero_same_relation(left, right)
        || relation_exists_or_count_zero_same_relation(right, left)
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

pub(super) fn relation_exists_or_count_zero_same_relation(
    exists_expr: &Expr,
    zero_expr: &Expr,
) -> bool {
    let Some((RelationAggregateKind::Exists, exists_relation, exists_select, exists_default)) =
        exists_expr.as_relation_aggregate()
    else {
        return false;
    };
    if exists_select.and_then(|expr| expr.as_bool()) != Some(true) || exists_default.is_some() {
        return false;
    }
    let Some(zero_relation) = relation_count_zero_relation(zero_expr) else {
        return false;
    };
    relation_cardinality_input_matches(exists_relation, zero_relation)
}

pub(super) fn relation_exists_or_not_exists_same_relation(
    exists_expr: &Expr,
    not_exists_expr: &Expr,
) -> bool {
    let Some((RelationAggregateKind::Exists, exists_relation, exists_select, exists_default)) =
        exists_expr.as_relation_aggregate()
    else {
        return false;
    };
    if exists_select.and_then(|expr| expr.as_bool()) != Some(true) || exists_default.is_some() {
        return false;
    }
    let Some((UnaryOpKind::Not, not_exists_inner)) = not_exists_expr.as_unary_op() else {
        return false;
    };
    let Some((
        RelationAggregateKind::Exists,
        not_exists_relation,
        not_exists_select,
        not_exists_default,
    )) = not_exists_inner.as_relation_aggregate()
    else {
        return false;
    };
    if not_exists_select.and_then(|expr| expr.as_bool()) != Some(true)
        || not_exists_default.is_some()
    {
        return false;
    }
    relation_cardinality_input_matches(exists_relation, not_exists_relation)
}

pub(super) fn relation_exists_contradicts_count_zero(
    exists_expr: &Expr,
    count_zero_expr: &Expr,
) -> bool {
    let Some((RelationAggregateKind::Exists, exists_relation, exists_select, exists_default)) =
        exists_expr.as_relation_aggregate()
    else {
        return false;
    };
    if exists_select.and_then(|expr| expr.as_bool()) != Some(true) || exists_default.is_some() {
        return false;
    }
    let Some(count_zero_relation) = relation_count_zero_relation(count_zero_expr) else {
        return false;
    };
    relation_cardinality_input_matches(exists_relation, count_zero_relation)
}

pub(super) fn relation_count_nonzero_implies_exists(left: &Expr, right: &Expr) -> bool {
    let Some(left_relation) =
        relation_count_nonzero_relation(left).or_else(|| relation_count_positive_lower_bound(left))
    else {
        return false;
    };
    let Some((RelationAggregateKind::Exists, right_relation, right_select, right_default)) =
        right.as_relation_aggregate()
    else {
        return false;
    };
    if right_select.and_then(|expr| expr.as_bool()) != Some(true) || right_default.is_some() {
        return false;
    }
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
        op = match op {
            BinOpKind::Lt => BinOpKind::Gt,
            BinOpKind::Le => BinOpKind::Ge,
            BinOpKind::Eq => BinOpKind::Eq,
            _ => return None,
        };
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

pub(super) fn relation_source_guard_candidates(
    expr: &Expr,
    known_relation_bases: &[String],
) -> Vec<String> {
    let mut candidates = relation_nonempty_source_names(expr)
        .into_iter()
        .filter(|source| {
            known_relation_bases.iter().any(|known| known == source)
                && !expr_references_relation_source_row(expr, source)
        })
        .collect::<Vec<_>>();

    if let Some(source) = relation_source_non_null_guard(expr) {
        if !candidates.iter().any(|existing| existing == &source) {
            candidates.push(source);
        }
    }

    candidates
}

pub(super) fn relation_nonempty_source_names(expr: &Expr) -> Vec<String> {
    let mut names = Vec::new();
    if let Some(name) = relation_count_nonzero_relation(expr)
        .and_then(relation_base_name)
        .map(str::to_string)
    {
        names.push(name);
    }
    if let Some(name) = relation_exists_true_relation(expr)
        .and_then(relation_base_name)
        .map(str::to_string)
    {
        if !names.iter().any(|existing| existing == &name) {
            names.push(name);
        }
    }
    names
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

pub(super) fn single_public_var_name(expr: &Expr) -> Option<String> {
    let mut vars = expr.vars().iter();
    let first = vars.next()?.clone();
    if vars.next().is_none() {
        Some(first)
    } else {
        None
    }
}

pub(super) fn relation_source_non_null_guard(expr: &Expr) -> Option<String> {
    let ExprKind::BinOp { op, left, right } = expr.kind() else {
        return None;
    };
    if *op != BinOpKind::Ne {
        return None;
    }
    relation_source_non_null_side(left, right)
        .or_else(|| relation_source_non_null_side(right, left))
}

pub(super) fn relation_source_non_null_side<'a>(
    maybe_source: &'a Expr,
    maybe_null: &Expr,
) -> Option<String> {
    if maybe_null.as_lit() != Some(&Lit::Null) {
        return None;
    }
    relation_source_expr_name(maybe_source)
}

pub(super) fn expr_references_relation_source_row(expr: &Expr, source_name: &str) -> bool {
    let row_prefix = format!("{source_name}.");
    expr_real_vars(expr)
        .iter()
        .any(|var| var.starts_with(&row_prefix) && var != source_name)
}

pub(super) fn expr_real_vars(expr: &Expr) -> Vec<String> {
    let mut out = Vec::new();
    collect_expr_real_vars(expr, &mut out);
    out.sort();
    out.dedup();
    out
}

pub(super) fn collect_expr_real_vars(expr: &Expr, out: &mut Vec<String>) {
    match expr.kind() {
        ExprKind::SymVar { origin } => {
            out.push(match origin {
                SymVarOrigin::Parameter { name, .. } => name.clone(),
                SymVarOrigin::External {
                    binding_id, path, ..
                } => {
                    if path.is_empty() {
                        format!("$e_{binding_id}")
                    } else {
                        format!("$e_{}.{}", binding_id, path.join("."))
                    }
                }
                SymVarOrigin::Temporal { binding_id, .. } => format!("$t_{binding_id}"),
            });
        }
        ExprKind::RowValue { alias } => out.push(alias.clone()),
        ExprKind::Call { args, .. } | ExprKind::ArrayLit(args) => {
            for arg in args {
                collect_expr_real_vars(arg, out);
            }
        }
        ExprKind::BinOp { left, right, .. } => {
            collect_expr_real_vars(left, out);
            collect_expr_real_vars(right, out);
        }
        ExprKind::UnaryOp { operand, .. } => collect_expr_real_vars(operand, out),
        ExprKind::CaseWhen { branches } => {
            for (cond, value) in branches {
                collect_expr_real_vars(cond, out);
                collect_expr_real_vars(value, out);
            }
        }
        ExprKind::RelationAggregate {
            relation,
            select_expr,
            default_expr,
            ..
        } => {
            collect_relation_real_vars(relation, out);
            if let Some(expr) = select_expr {
                collect_expr_real_vars(expr, out);
            }
            if let Some(expr) = default_expr {
                collect_expr_real_vars(expr, out);
            }
        }
        ExprKind::Relation(relation) => collect_relation_real_vars(relation, out),
        ExprKind::Lit(_) | ExprKind::HeapRef(_) => {}
    }
}

pub(super) fn collect_relation_real_vars(
    relation: &crate::domains::relation::RelationExpr,
    out: &mut Vec<String>,
) {
    match relation {
        crate::domains::relation::RelationExpr::Empty
        | crate::domains::relation::RelationExpr::Base { .. } => {}
        crate::domains::relation::RelationExpr::Vector { value, .. } => {
            collect_expr_real_vars(value, out);
        }
        crate::domains::relation::RelationExpr::Select { predicate, input } => {
            collect_expr_real_vars(predicate, out);
            collect_relation_real_vars(input, out);
        }
        crate::domains::relation::RelationExpr::Project { value, input, .. } => {
            collect_expr_real_vars(value, out);
            collect_relation_real_vars(input, out);
        }
        crate::domains::relation::RelationExpr::Distinct(input) => {
            collect_relation_real_vars(input, out);
        }
    }
}

pub(super) fn relation_base_name(
    relation: &crate::domains::relation::RelationExpr,
) -> Option<&str> {
    match relation {
        crate::domains::relation::RelationExpr::Base { name, .. } => Some(name.as_str()),
        crate::domains::relation::RelationExpr::Select { input, .. } | crate::domains::relation::RelationExpr::Project { input, .. } | crate::domains::relation::RelationExpr::Distinct(input) => relation_base_name(input),
        crate::domains::relation::RelationExpr::Empty | crate::domains::relation::RelationExpr::Vector { .. } => None,
    }
}

pub(super) fn relation_source_expr_name(expr: &Expr) -> Option<String> {
    if let Some(origin) = expr.as_sym_var() {
        return match origin {
            SymVarOrigin::Parameter { name, .. } => Some(name.clone()),
            SymVarOrigin::External {
                binding_id, path, ..
            } => {
                if path.is_empty() {
                    Some(format!("$e_{binding_id}"))
                } else {
                    Some(format!("$e_{}.{}", binding_id, path.join(".")))
                }
            }
            SymVarOrigin::Temporal { binding_id, .. } => Some(format!("$t_{binding_id}")),
        };
    }

    // Relation-backed source guards may be represented by a HeapRef/object
    // carrier whose public display name is carried in the expression profile
    // vars.  This narrow fallback is only used for `source != null` guards and
    // is later accepted only when a matching relation aggregate base exists in
    // the same conjunction.
    single_public_var_name(expr)
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
