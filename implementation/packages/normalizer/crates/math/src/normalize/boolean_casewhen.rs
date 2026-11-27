//! MA-04 / CF-10: expand a complete Boolean CASE only when every
//! inspected condition and result is a total value in the current scope.
//! A Boolean result type alone does not establish receipt availability.

use super::logical_chain::{rebuild_and, rebuild_or};
use crate::expr::{BinOpKind, Expr, ExprKind, Lit, UnaryOpKind};

pub(super) fn total_value(expr: &Expr) -> bool {
    match expr.kind() {
        ExprKind::Lit(_) => true,
        ExprKind::SymVar { origin: crate::SymVarOrigin::Parameter { name, primitive_type } } =>
            !name.starts_with('$') && !name.contains(['.', '[', ']'])
                && matches!(primitive_type.as_str(), "boolean" | "integer" | "text" | "text?"),
        ExprKind::UnaryOp { operand, .. } => total_value(operand),
        ExprKind::BinOp { left, right, .. } => total_value(left) && total_value(right),
        ExprKind::CaseWhen { .. } => expr.as_total_case_when().is_some_and(|(arms, fallback)| {
            total_value(fallback) && arms.iter().all(|(guard, value)| total_value(guard) && total_value(value))
        }),
        // An acquisition, a lookup, or a pure-but-partial call must stay
        // within the source-selected arm unless the caller proves otherwise.
        _ => false,
    }
}

pub(crate) fn simplify(expr: &Expr, ctx: &dyn crate::SortContext) -> Expr {
    // Only a complete Boolean value can become a total OR expression. A
    // partial CASE must remain undefined outside its selected domain.
    let Some((arms, fallback)) = expr.as_total_case_when() else {
        return expr.clone();
    };
    if !total_value(expr) || !arms.iter().all(|(_, value)| is_boolean_expr(value))
        || !is_boolean_expr(fallback)
    {
        return expr.clone();
    }

    // 展开为 OR-of-AND：每个 branch 生成一个 AND 项 (path_cond AND value)
    // 其中 path_cond = 前面 branches.cond 全部 NOT 后 AND 当前 cond（若 cond 是 true，省略当前 cond）
    let mut or_terms: Vec<Expr> = Vec::with_capacity(arms.len() + 1);
    let mut accumulated_negation: Vec<Expr> = Vec::new();

    for (guard, value) in arms.iter().map(|(guard, value)| (Some(guard), value))
        .chain(std::iter::once((None, fallback)))
    {
        let guard = guard.filter(|guard| guard.as_bool() != Some(true));
        let mut conjuncts: Vec<Expr> = accumulated_negation.clone();
        if let Some(guard) = guard {
            conjuncts.push(guard.clone());
        }
        conjuncts.push(value.clone());
        or_terms.push(rebuild_and(&conjuncts));

        // An earlier TRUE arm masks every later arm, including the fallback.
        let Some(guard) = guard else { break; };
        accumulated_negation.push(crate::builder::not(guard.clone()));
    }

    let candidate = rebuild_or(&or_terms);
    // Duplicated guards may themselves own CASE arms. Count the entire
    // rebuilt tree; removing the outer CASE alone does not prove progress.
    if crate::canonical_order_compare(&candidate, expr, ctx).is_lt() {
        candidate
    } else {
        expr.clone()
    }
}

pub(crate) fn is_boolean_expr(expr: &Expr) -> bool {
    if expr.contains_relation_aggregate() {
        return false;
    }
    match expr.kind() {
        ExprKind::Lit(Lit::Bool(_)) => true,
        ExprKind::BinOp { op, .. } =>
            matches!(op, BinOpKind::And | BinOpKind::Or) || op.swapped_comparison().is_some(),
        ExprKind::UnaryOp {
            op: UnaryOpKind::Not,
            ..
        } => true,
        ExprKind::SymVar { origin } => origin.primitive_type() == "boolean",
        ExprKind::Call { key, .. } =>
            crate::domains::primitive::registry::func_return_type(*key)
                == Some(crate::Primitive::Boolean),
        // Relation aggregates are lazy observations over a source domain. A
        // CASE selecting one of them must remain a CASE so branch selection
        // does not become eager AND/OR evaluation on an unselected relation.
        ExprKind::CaseWhen { .. } => expr.as_total_case_when().is_some_and(|(arms, fallback)| {
            arms.iter().all(|(_, value)| is_boolean_expr(value)) && is_boolean_expr(fallback)
        }),
        _ => false,
    }
}
