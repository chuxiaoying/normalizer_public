//! Keep arithmetic around CASE unless bounded arm-local reduction makes the
//! entire value smaller. Two independent CASEs never form a cross product.

use crate::{
    builder as b, expr::{Expr, ExprKind}, BinOpKind, SortContext, SymVarOrigin, UnaryOpKind,
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
        return expr.clone();
    }

    if matches!(op, BinOpKind::Add | BinOpKind::Sub | BinOpKind::Mul) {
        return reduce_integer_case(expr, *op, left, right, ctx)
            .unwrap_or_else(|| expr.clone());
    }

    let (case, branches, other, case_on_left) = match (left_case, right_case) {
        (Some(branches), None) => (left, branches, right, true),
        (None, Some(branches)) => (right, branches, left, false),
        _ => return expr.clone(),
    };
    if !is_leaf(other)
        || branches.len() <= 4 && (case.size() > 2048 || case.depth() > 48)
    {
        return expr.clone();
    }
    let branches = branches.iter().map(|(guard, value)| {
        let (a, b) = if case_on_left { (value, other) } else { (other, value) };
        (guard.clone(), Expr::new(ExprKind::BinOp {
            op: *op, left: a.clone(), right: b.clone(),
        }))
    }).collect();
    Expr::new(ExprKind::CaseWhen { branches })
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
    if [case, fixed].into_iter().any(|value| value.size() > 2048 || value.depth() > 48)
        || !acquired_integer(fixed)
        || !branches.iter().all(|(guard, value)| acquired_guard(guard) && acquired_integer(value))
    {
        return None;
    }
    let projected_nodes = case.size().saturating_add(
        branches.len().saturating_mul(1usize.saturating_add(fixed.size())),
    );
    if !super::consume_candidate_budget(projected_nodes, projected_nodes) {
        return None;
    }
    let mut result = branches.iter().map(|(guard, value)| {
        let (a, b) = if case_on_left { (value, fixed) } else { (fixed, value) };
        let operation = b::binop_public(op, a.clone(), b.clone());
        // These are local, terminating rules, not a new normalization scope.
        let folded = super::constant::fold(&operation);
        let reduced = super::arithmetic::simplify(&folded);
        (guard.clone(), super::integer_ac::simplify(&reduced, ctx))
    }).collect::<Vec<_>>();
    let last = result.pop()?;
    let candidate = b::normal_return_case_when(result, last);
    (cost(&candidate) < cost(expr)).then_some(candidate)
}

fn cost(value: &Expr) -> (usize, usize) {
    let mut arms = 0usize;
    let mut operations = 0usize;
    // walk counts occurrences, not Arc/DAG nodes. Duplicating an operator
    // in two arms is still two operations in the delivered representation.
    value.walk(&mut |e| match e.kind() {
        ExprKind::CaseWhen { branches } => arms = arms.saturating_add(branches.len()),
        ExprKind::BinOp { .. } | ExprKind::UnaryOp { .. } | ExprKind::Call { .. } =>
            operations = operations.saturating_add(1),
        _ => (),
    });
    (arms, operations)
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
            && arms.iter().all(|(guard, value)| acquired_guard(guard) && acquired_integer(value));
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
    if let Some(SymVarOrigin::Parameter { name, primitive_type }) = value.as_sym_var() {
        return primitive_type == "boolean" && acquired_parameter(name);
    }
    if let Some((UnaryOpKind::Not, operand)) = value.as_unary_op() {
        return acquired_guard(operand);
    }
    match value.as_binop() {
        Some((BinOpKind::And | BinOpKind::Or, left, right)) =>
            acquired_guard(left) && acquired_guard(right),
        Some((op, left, right)) if op.swapped_comparison().is_some() =>
            acquired_integer(left) && acquired_integer(right),
        _ => false,
    }
}

fn is_leaf(expr: &Expr) -> bool {
    matches!(expr.kind(), ExprKind::Lit(_) | ExprKind::SymVar { .. } | ExprKind::HeapRef(_))
}

#[cfg(test)]
mod distribution_boundary_tests {
    use super::*;
    use crate::{DefaultSortContext, ScalarSymbolType};

    fn simplify_here(expression: &Expr) -> Expr {
        simplify(expression, &DefaultSortContext)
    }

    #[test]
    fn whole_value_complexity_drives_numeric_case_distribution() {
        let p = b::param("p", ScalarSymbolType::Boolean);
        let x = b::param("x", ScalarSymbolType::Integer);
        let z = b::param("z", ScalarSymbolType::Integer);
        let two = b::lit_int(2);
        let source = |value| b::case_when(vec![(p.clone(), value)], b::lit_int(7));
        let left = b::sub(source(two.clone()), z.clone());
        let right = b::sub(z.clone(), source(two.clone()));
        let both = b::sub(source(x.clone()), source(z));
        for unchanged in [left, right, both] {
            assert_eq!(simplify_here(&unchanged), unchanged);
        }

        let borrowed = b::sym_e(0, ScalarSymbolType::Integer);
        let cannot_cancel = b::sub(source(borrowed.clone()), borrowed);
        assert_eq!(simplify_here(&cannot_cancel), cannot_cancel);

        let cancel = b::sub(
            b::case_when(vec![(p.clone(), x.clone())], b::add(x.clone(), two.clone())),
            x,
        );
        let result = b::case_when(vec![(p.clone(), b::lit_int(0))], two);
        assert_eq!(simplify_here(&cancel), result);
        assert_eq!(simplify_here(&result), result);
        assert_eq!(simplify_here(&b::sub(result.clone(), result)), b::lit_int(0));

        let partial = b::normal_return_case_when(vec![], (p, b::lit_int(7)));
        let same_partial = b::sub(partial.clone(), partial);
        assert_eq!(simplify_here(&same_partial), same_partial);
    }

    #[test]
    fn partial_case_keeps_its_guard_and_operand_roles() {
        let p = b::param("p", ScalarSymbolType::Boolean);
        let partial = b::normal_return_case_when(vec![], (p.clone(), b::lit_int(7)));
        let subtraction = b::lit_int(2);
        let from_case = simplify_here(&b::sub(partial.clone(), subtraction.clone()));
        let into_case = simplify_here(&b::sub(subtraction, partial));
        assert_eq!(from_case, b::normal_return_case_when(vec![], (p.clone(), b::lit_int(5))));
        assert_eq!(into_case, b::normal_return_case_when(vec![], (p, b::lit_int(-5))));
        assert!(from_case.as_total_case_when().is_none());
        assert!(into_case.as_total_case_when().is_none());
    }

    #[test]
    fn case_distribution_preserves_both_growth_ceilings() {
        let p = b::param("p", ScalarSymbolType::Boolean);
        let x = b::param("x", ScalarSymbolType::Integer);
        let zero = b::lit_int(0);
        let mut wide = x.clone();
        while wide.size() <= 2048 {
            wide = b::add(wide.clone(), wide);
        }
        let deep = (0..49).fold(x.clone(), |value, _| b::sub(value, b::lit_int(1)));

        let case = |value| b::case_when(vec![(p.clone(), value)], zero.clone());
        let wide_case = case(wide);
        let deep_case = case(deep);
        assert!(wide_case.depth() <= 48 && wide_case.size() > 2048);
        assert!(deep_case.size() <= 2048 && deep_case.depth() > 48);
        for protected in [wide_case, deep_case] {
            let composed = b::add(protected, b::lit_int(3));
            assert_eq!(simplify_here(&composed), composed);
        }
    }
}
