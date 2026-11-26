//! 线性约束域求解器。
//!
//! 本模块为 AND 上下文提供体系化的**同 subject 算术约束矛盾/蕴含判定**，
//! 替代零散的单点规则（如 `x < x → false`）。
//!
//! ## 设计
//!
//! 对 `x op c` 形式的比较表达式（x 为任意 Expr subject，op 为比较运算符，
//! c 为整数字面量），提取为 `LinearConstraint { subject, bound }`。
//!
//! 给定 AND 上下文中的约束集合，对每个 subject 构建一维整数域，
//! 检测新约束是否与已知域矛盾（恒 false）或被蕴含（恒 true）。
//!
//! ## 不变式
//!
//! - 不调用 `path_implies` / `path_contradicts`（避免递归）
//! - Subject 识别使用 Expr structural equality（`==`）
//! - O(n) 扫描 AND 子式，无递归风险
//! - 只处理整数字面量 bound；非字面量 bound 不参与求解

use crate::expr::{BinOpKind, Expr, ExprKind, Lit};

/// 约束的 bound 部分。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConstraintBound {
    /// subject > value
    Gt(i64),
    /// subject >= value
    Ge(i64),
    /// subject < value
    Lt(i64),
    /// subject <= value
    Le(i64),
    /// subject == value
    Eq(i64),
    /// subject != value
    Ne(i64),
}

/// 从比较表达式中提取的线性约束。
#[derive(Debug, Clone)]
pub(crate) struct LinearConstraint {
    pub subject: Expr,
    pub bound: ConstraintBound,
}

/// Align a recognized literal on the right without widening any caller's policy.
pub(super) fn comparison_literal_rhs<'a, T>(
    (op, left, right): (BinOpKind, &'a Expr, &'a Expr),
    literal: impl Fn(&Expr) -> Option<T>,
) -> Option<(BinOpKind, &'a Expr, T)> {
    let swapped = op.swapped_comparison()?;
    if let Some(value) = literal(right) {
        Some((op, left, value))
    } else {
        literal(left).map(|value| (swapped, right, value))
    }
}

pub(super) fn compact_integer(expr: &Expr) -> Option<i64> {
    match expr.as_lit()? {
        Lit::Int(value) => Some(*value),
        _ => None,
    }
}

/// 从比较表达式中提取线性约束。
///
/// 支持的形式：
/// - `x op c`（x 为任意非字面量 Expr，c 为整数字面量）
/// - `c op x`（自动翻转为 `x flip(op) c`）
///
/// 返回 None 如果表达式不是上述形式。
pub(crate) fn extract_linear_constraint(expr: &Expr) -> Option<LinearConstraint> {
    let (subject, bound) = compact_bound(expr)?;
    // An ordinary local context proof requires a nonliteral subject.
    subject.as_lit().is_none().then(|| LinearConstraint {
        subject: subject.clone(),
        bound,
    })
}

fn compact_bound(expr: &Expr) -> Option<(&Expr, ConstraintBound)> {
    let (op, subject, value) = comparison_literal_rhs(expr.as_binop()?, compact_integer)?;
    use BinOpKind::*;
    let bound = match op {
        Lt => ConstraintBound::Lt(value),
        Gt => ConstraintBound::Gt(value),
        Le => ConstraintBound::Le(value),
        Ge => ConstraintBound::Ge(value),
        Eq => ConstraintBound::Eq(value),
        Ne => ConstraintBound::Ne(value),
        _ => return None,
    };
    Some((subject, bound))
}

/// 检查两个同 subject 约束是否矛盾（域交集为空）。
///
/// 前提：调用方已确认 `a.subject == b.subject`。
pub(crate) fn bounds_contradict(a: ConstraintBound, b: ConstraintBound) -> bool {
    use ConstraintBound::*;
    match (a, b) {
        (Eq(point), bound) | (bound, Eq(point)) => !point_satisfies(bound, point),
        (Gt(lo), Lt(hi)) | (Lt(hi), Gt(lo))
        | (Gt(lo), Le(hi)) | (Le(hi), Gt(lo))
        | (Ge(lo), Lt(hi)) | (Lt(hi), Ge(lo)) => hi <= lo,
        (Ge(lo), Le(hi)) | (Le(hi), Ge(lo)) => hi < lo,
        _ => false,
    }
}

fn point_satisfies(bound: ConstraintBound, point: i64) -> bool {
    use ConstraintBound::*;
    match bound {
        Gt(value) => point > value,
        Ge(value) => point >= value,
        Lt(value) => point < value,
        Le(value) => point <= value,
        Eq(value) => point == value,
        Ne(value) => point != value,
    }
}

/// 检查 `existing` 是否蕴含 `target`（在 existing 成立时 target 恒 true）。
///
/// 前提：调用方已确认 subject 相同。
pub(crate) fn bound_implies(existing: ConstraintBound, target: ConstraintBound) -> bool {
    use ConstraintBound::*;
    match (existing, target) {
        (Eq(point), required) => point_satisfies(required, point),
        (Ne(_), _) => false,
        (known, Ne(point)) => !point_satisfies(known, point),
        (Gt(a), Gt(b) | Ge(b)) | (Ge(a), Ge(b)) => a >= b,
        (Ge(a), Gt(b)) => a > b,
        (Lt(a), Lt(b)) | (Le(a), Le(b)) => a <= b,
        (Lt(a), Le(b)) => i128::from(a) <= i128::from(b) + 1,
        (Le(a), Lt(b)) => a < b,
        _ => false,
    }
}

fn any_matching_bound(
    expr: &Expr,
    context: &[Expr],
    proves: impl Fn(ConstraintBound, ConstraintBound) -> bool,
) -> bool {
    let Some(target) = extract_linear_constraint(expr) else { return false; };
    context.iter().filter_map(extract_linear_constraint).any(|candidate| {
        candidate.subject == target.subject && proves(candidate.bound, target.bound)
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
