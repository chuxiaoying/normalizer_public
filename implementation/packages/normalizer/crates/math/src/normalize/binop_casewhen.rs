//! Distribute a pure binary operation through one CASE without changing operand order.
//! The other operand must be a leaf; two CASEs never form a cross product here.

use crate::expr::{Expr, ExprKind};

pub(crate) fn simplify(expr: &Expr) -> Expr {
    let ExprKind::BinOp { op, left, right } = expr.kind() else {
        return expr.clone();
    };

    let left_case = left.as_case_when();
    let right_case = right.as_case_when();
    if left_case.is_some() && right_case.is_some() {
        return expr.clone();
    }

    // When a branch selected the subtrahend itself, the corresponding
    // difference is exactly zero. Distribute only if this actually shrinks
    // the tree; never expand two CASE expressions into a cross product.
    if *op == crate::BinOpKind::Sub && super::integer_ac::total_integer(right) {
        if let Some(branches) = left_case {
            if branches.iter().any(|(_, value)| value == right) {
                let candidate = crate::builder::normal_return_case_when(
                    branches
                        .iter()
                        .map(|(guard, value)| {
                            let delta = if value == right {
                                crate::builder::lit_int(0)
                            } else {
                                crate::builder::sub(value.clone(), right.clone())
                            };
                            (guard.clone(), delta)
                        })
                        .collect(),
                );
                if candidate.size() < expr.size() {
                    return candidate;
                }
            }
        }
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

fn is_leaf(expr: &Expr) -> bool {
    matches!(expr.kind(), ExprKind::Lit(_) | ExprKind::SymVar { .. } | ExprKind::HeapRef(_))
}
