//! Keep arithmetic around CASE unless bounded arm-local reduction makes the
//! entire value smaller. Two independent CASEs never form a cross product.

use crate::{
    builder as b,
    expr::{Expr, ExprKind},
    BinOpKind, SortContext, SymVarOrigin, UnaryOpKind,
};

pub(crate) fn simplify(expr: &Expr, ctx: &dyn SortContext) -> Expr {
    let ExprKind::BinOp { op, left, right } = expr.kind() else {
        return expr.clone();
    };

    let left_case = left.as_case_when();
    let right_case = right.as_case_when();
    if left_case.is_some() && right_case.is_some() {
        // Identity cancellation is not a cross product. Check the whole
        // acquired value, including every guard and a real fallback: two
        // identical partial CASEs still have an unobserved domain.
        if *op == BinOpKind::Sub
            && left.size() <= 2048
            && left.depth() <= 48
            && left == right
            && acquired_integer(left)
        {
            return b::lit_int(0);
        }
        if matches!(op, BinOpKind::Add | BinOpKind::Mul)
            && left == right
            && left.size() <= 2048
            && left.depth() <= 48
            && acquired_integer(left)
        {
            if let Some(([(guard, then_value)], else_value)) = left.as_total_case_when() {
                let nodes = left.size().saturating_mul(2).saturating_add(2);
                if super::consume_candidate_budget(nodes, nodes) {
                    let run = |value: &Expr| {
                        let applied = b::binop_public(*op, value.clone(), value.clone());
                        let folded = super::constant::fold(&applied);
                        let reduced = super::arithmetic::simplify(&folded, ctx);
                        super::integer_ac::simplify(&reduced, ctx)
                    };
                    let candidate =
                        b::case_when(vec![(guard.clone(), run(then_value))], run(else_value));
                    if crate::canonical_order::canonical_cost_compare(&candidate, expr, ctx).is_lt()
                    {
                        return candidate;
                    }
                }
            }
        }
        return expr.clone();
    }

    if matches!(op, BinOpKind::Add | BinOpKind::Sub | BinOpKind::Mul) || op.comparison().is_some() {
        return reduce_integer_case(expr, *op, left, right, ctx).unwrap_or_else(|| expr.clone());
    }
    expr.clone()
}

/// A rejected candidate never escapes this local phase. Its guards and first-match
/// order are identical to the original CASE, even when the latter is partial.
fn reduce_integer_case(
    expr: &Expr,
    op: BinOpKind,
    left: &Expr,
    right: &Expr,
    ctx: &dyn SortContext,
) -> Option<Expr> {
    let (case, fixed, case_on_left) = match (left.as_case_when(), right.as_case_when()) {
        (Some(_), None) => (left, right, true),
        (None, Some(_)) => (right, left, false),
        _ => return None,
    };
    let branches = case.as_case_when()?;
    if [case, fixed]
        .into_iter()
        .any(|value| value.size() > 2048 || value.depth() > 48)
        || !acquired_integer(fixed)
        || !branches
            .iter()
            .all(|(guard, value)| acquired_guard(guard) && acquired_integer(value))
    {
        return None;
    }
    let projected_nodes = case.size().saturating_add(
        branches
            .len()
            .saturating_mul(1usize.saturating_add(fixed.size())),
    );
    if !super::consume_candidate_budget(projected_nodes, projected_nodes) {
        return None;
    }
    let mut result = branches
        .iter()
        .map(|(guard, value)| {
            let (a, b) = if case_on_left {
                (value, fixed)
            } else {
                (fixed, value)
            };
            let operation = b::binop_public(op, a.clone(), b.clone());
            // These are local, terminating rules, not a new normalization scope.
            let folded = super::constant::fold(&operation);
            let reduced = super::arithmetic::simplify(&folded, ctx);
            let reduced = if op.comparison().is_some() {
                super::integer_ranges::strict_endpoint(&reduced).unwrap_or(reduced)
            } else {
                super::integer_ac::simplify(&reduced, ctx)
            };
            (guard.clone(), reduced)
        })
        .collect::<Vec<_>>();
    let last = result.pop()?;
    let candidate = b::normal_return_case_when(result, last);
    crate::canonical_order::canonical_cost_compare(&candidate, expr, ctx)
        .is_lt()
        .then_some(candidate)
}

fn acquired_parameter(name: &str) -> bool {
    // Checked source fields use P paths (e.g. request.value) after their
    // evaluation; internal state, staged host observations and position
    // witnesses are not proof that another path may read the value.
    !name.starts_with('$') && !name.contains(['[', ']'])
}

fn acquired_integer(value: &Expr) -> bool {
    if let Some((arms, fallback)) = value.as_total_case_when() {
        return acquired_integer(fallback)
            && arms
                .iter()
                .all(|(guard, value)| acquired_guard(guard) && acquired_integer(value));
    }
    if !super::integer_ac::total_integer(value) {
        return false;
    }
    // The host uses Parameter storage for staged reads too.
    let mut acquired = true;
    value.walk(&mut |expr| {
        if let Some(SymVarOrigin::Parameter { name, .. }) = expr.as_sym_var() {
            acquired &= acquired_parameter(name);
        }
    });
    acquired
}

fn acquired_guard(value: &Expr) -> bool {
    if value.as_bool().is_some() {
        return true;
    }
    if let Some(SymVarOrigin::Parameter {
        name,
        primitive_type,
    }) = value.as_sym_var()
    {
        return primitive_type == "boolean" && acquired_parameter(name);
    }
    if let Some((UnaryOpKind::Not, operand)) = value.as_unary_op() {
        return acquired_guard(operand);
    }
    match value.as_binop() {
        Some((BinOpKind::And | BinOpKind::Or, left, right)) => {
            acquired_guard(left) && acquired_guard(right)
        }
        Some((op, left, right)) if op.swapped_comparison().is_some() => {
            acquired_integer(left) && acquired_integer(right)
        }
        _ => false,
    }
}
