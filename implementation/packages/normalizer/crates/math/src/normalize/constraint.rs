//! Exact bound proofs for an acquired integer subject. Strict bounds
//! become inclusive endpoints without using a machine-word limit.

use crate::expr::{BinOpKind, Comparison, Expr, ExprKind};
use num_bigint::BigInt;

#[derive(Debug, Clone, PartialEq, Eq)]
enum ConstraintBound {
    Interval { low: Option<BigInt>, high: Option<BigInt> },
    Excludes(BigInt),
}

#[derive(Debug, Clone)]
struct LinearConstraint {
    subject: Expr,
    bound: ConstraintBound,
}

/// Align a recognized literal on the right without widening any caller's policy.
pub(super) fn comparison_literal_rhs<'a, T>(
    (op, left, right): (BinOpKind, &'a Expr, &'a Expr),
    literal: impl Fn(&Expr) -> Option<T>,
) -> Option<(BinOpKind, &'a Expr, T)> {
    let (op, subject, value) = align_comparison_literal(op.comparison()?, left, right, literal)?;
    Some((op.into(), subject, value))
}

fn align_comparison_literal<'a, T>(
    op: Comparison,
    left: &'a Expr,
    right: &'a Expr,
    literal: impl Fn(&Expr) -> Option<T>,
) -> Option<(Comparison, &'a Expr, T)> {
    if let Some(value) = literal(right) {
        Some((op, left, value))
    } else {
        literal(left).map(|value| (op.swapped(), right, value))
    }
}

pub(super) fn integer_literal(expr: &Expr) -> Option<BigInt> {
    expr.as_lit()?.as_integer()
}

fn extract_linear_constraint(expr: &Expr) -> Option<LinearConstraint> {
    let (op, left, right) = expr.as_binop()?;
    let (op, subject, value) =
        align_comparison_literal(op.comparison()?, left, right, integer_literal)?;
    if subject.as_lit().is_some() || !super::integer_ac::total_integer(subject) {
        return None;
    }
    use ConstraintBound::{Excludes, Interval};
    let bound = match op {
        Comparison::Gt => Interval { low: Some(value + 1), high: None },
        Comparison::Ge => Interval { low: Some(value), high: None },
        Comparison::Lt => Interval { low: None, high: Some(value - 1) },
        Comparison::Le => Interval { low: None, high: Some(value) },
        Comparison::Eq => Interval { low: Some(value.clone()), high: Some(value) },
        Comparison::Ne => Excludes(value),
    };
    Some(LinearConstraint { subject: subject.clone(), bound })
}

fn bounds_contradict(a: &ConstraintBound, b: &ConstraintBound) -> bool {
    use ConstraintBound::{Excludes, Interval};
    match (a, b) {
        (Interval { low: a_low, high: a_high }, Interval { low: b_low, high: b_high }) => {
            a_low.as_ref().zip(b_high.as_ref()).is_some_and(|(low, high)| low > high)
                || b_low.as_ref().zip(a_high.as_ref()).is_some_and(|(low, high)| low > high)
        }
        (Interval { low, high }, Excludes(value)) | (Excludes(value), Interval { low, high }) =>
            low.as_ref() == Some(value) && high.as_ref() == Some(value),
        (Excludes(_), Excludes(_)) => false,
    }
}

fn bound_implies(existing: &ConstraintBound, target: &ConstraintBound) -> bool {
    use ConstraintBound::{Excludes, Interval};
    match (existing, target) {
        (Interval { low: a_low, high: a_high }, Interval { low: b_low, high: b_high }) => {
            b_low.as_ref().is_none_or(|low| a_low.as_ref().is_some_and(|own| own >= low))
                && b_high.as_ref().is_none_or(|high| a_high.as_ref().is_some_and(|own| own <= high))
        }
        (Interval { low, high }, Excludes(value)) =>
            low.as_ref().is_some_and(|low| value < low) || high.as_ref().is_some_and(|high| value > high),
        (Excludes(a), Excludes(b)) => a == b,
        (Excludes(_), Interval { .. }) => false,
    }
}

fn any_matching_bound(
    expr: &Expr,
    context: &[Expr],
    proves: impl Fn(&ConstraintBound, &ConstraintBound) -> bool,
) -> bool {
    let Some(target) = extract_linear_constraint(expr) else { return false; };
    context.iter().filter_map(extract_linear_constraint).any(|candidate| {
        candidate.subject == target.subject && proves(&candidate.bound, &target.bound)
    })
}

/// 在 AND 上下文中检查 `expr` 是否因线性约束矛盾而恒 false。
///
/// 从 `context` 提取所有 `subject op literal` 约束，然后检查 `expr`
/// 是否与某个同 subject 约束矛盾。
///
/// 如果 `expr` 本身是 AND/OR 复合式，递归检查子式。
pub(crate) fn expr_contradicts_linear_context(expr: &Expr, context: &[Expr]) -> bool {
    if any_matching_bound(expr, context, bounds_contradict) {
        return true;
    }

    // For OR expressions: if ALL disjuncts contradict, the whole OR is false
    if let ExprKind::BinOp {
        op: BinOpKind::Or,
        left,
        right,
    } = expr.kind()
    {
        if expr_contradicts_linear_context(left, context)
            && expr_contradicts_linear_context(right, context)
        {
            return true;
        }
    }

    false
}

/// 在 AND 上下文中检查 `expr` 是否因线性约束蕴含而恒 true。
///
/// 从 `context` 提取所有 `subject op literal` 约束，然后检查 `expr`
/// 是否被某个同 subject 约束蕴含。
pub(crate) fn expr_implied_by_linear_context(expr: &Expr, context: &[Expr]) -> bool {
    // The Boolean proof caller checks each leaf of a conjunction.
    any_matching_bound(expr, context, bound_implies)
}
