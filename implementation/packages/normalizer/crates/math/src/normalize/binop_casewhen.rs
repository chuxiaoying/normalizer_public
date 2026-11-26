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
        if let Some((last, earlier)) = left_case.and_then(|branches| branches.split_last()) {
            if earlier.iter().chain([last]).any(|(_, value)| value == right) {
                let subtract = |(guard, value): &(Expr, Expr)| {
                    let delta = if value == right {
                        crate::builder::lit_int(0)
                    } else {
                        crate::builder::sub(value.clone(), right.clone())
                    };
                    (guard.clone(), delta)
                };
                let candidate = crate::builder::normal_return_case_when(
                    earlier.iter().map(&subtract).collect(),
                    subtract(last),
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

#[cfg(test)]
mod distribution_boundary_tests {
    use super::*;
    use crate::{builder as b, ScalarSymbolType};

    #[test]
    fn case_distribution_preserves_two_independent_growth_ceilings() {
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
            assert_eq!(simplify(&composed), composed);
        }

        let small = b::add(case(x.clone()), b::lit_int(3));
        assert_eq!(
            simplify(&small),
            b::case_when(
                vec![(p, b::add(x, b::lit_int(3)))],
                b::add(zero, b::lit_int(3)),
            )
        );
    }

    #[test]
    fn left_case_subtraction_never_swaps_operand_roles_or_partiality() {
        let p = b::param("p", ScalarSymbolType::Boolean);
        let x = b::param("x", ScalarSymbolType::Integer);
        let partial = b::normal_return_case_when(vec![], (p.clone(), x.clone()));
        let subtrahend = b::lit_int(2);

        let from_case = simplify(&b::sub(partial.clone(), subtrahend.clone()));
        let into_case = simplify(&b::sub(subtrahend.clone(), partial));
        assert_eq!(from_case, b::normal_return_case_when(vec![], (
            p.clone(), b::sub(x.clone(), subtrahend.clone())
        )));
        assert_eq!(into_case, b::normal_return_case_when(vec![], (
            p, b::sub(subtrahend, x)
        )));
        assert!(from_case.as_total_case_when().is_none());
        assert!(into_case.as_total_case_when().is_none());
    }
}
