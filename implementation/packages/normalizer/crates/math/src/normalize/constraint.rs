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

/// 从比较表达式中提取线性约束。
///
/// 支持的形式：
/// - `x op c`（x 为任意非字面量 Expr，c 为整数字面量）
/// - `c op x`（自动翻转为 `x flip(op) c`）
///
/// 返回 None 如果表达式不是上述形式。
pub(crate) fn extract_linear_constraint(expr: &Expr) -> Option<LinearConstraint> {
    let (op, left, right) = expr.as_binop()?;
    let (op, subject, value) = match (left.as_lit(), right.as_lit()) {
        (None, Some(Lit::Int(n))) => (op, left, *n),
        (Some(Lit::Int(n)), None) => (op.swapped_comparison()?, right, *n),
        // Both literals are folded separately.
        _ => return None,
    };
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
    Some(LinearConstraint { subject: subject.clone(), bound })
}

/// 检查两个同 subject 约束是否矛盾（域交集为空）。
///
/// 前提：调用方已确认 `a.subject == b.subject`。
pub(crate) fn bounds_contradict(a: ConstraintBound, b: ConstraintBound) -> bool {
    // 对称性：检查 (a, b) 和 (b, a)
    check_ordered_contradiction(a, b) || check_ordered_contradiction(b, a)
}

/// 单向矛盾检查（不对称部分）。
fn check_ordered_contradiction(existing: ConstraintBound, new: ConstraintBound) -> bool {
    use ConstraintBound::*;

    match existing {
        // existing: subject > a
        Gt(a) => match new {
            Lt(b) => b <= a, // x > a && x < b: 矛盾当 b <= a  (e.g. x>4 && x<4, x>4 && x<3)
            Le(b) => b <= a, // x > a && x <= b: 矛盾当 b <= a (e.g. x>4 && x<=4)
            _ => false,
        },
        // existing: subject >= a
        Ge(a) => match new {
            Lt(b) => b <= a, // x >= a && x < b: 矛盾当 b <= a (e.g. x>=4 && x<4)
            Le(b) => b < a,  // x >= a && x <= b: 矛盾当 b < a (e.g. x>=4 && x<=3)
            _ => false,
        },
        // existing: subject < a
        Lt(a) => match new {
            Gt(b) => b >= a, // x < a && x > b: 矛盾当 b >= a (e.g. x<4 && x>4, x<4 && x>5)
            Ge(b) => b >= a, // x < a && x >= b: 矛盾当 b >= a (e.g. x<4 && x>=4)
            _ => false,
        },
        // existing: subject <= a
        Le(a) => match new {
            Gt(b) => b >= a, // x <= a && x > b: 矛盾当 b >= a (e.g. x<=4 && x>4)
            Ge(b) => b > a,  // x <= a && x >= b: 矛盾当 b > a (e.g. x<=4 && x>=5)
            _ => false,
        },
        // existing: subject == a
        Eq(a) => match new {
            Ne(b) => b == a, // x == a && x != a
            Eq(b) => b != a, // x == a && x == b (a != b)
            Gt(b) => a <= b, // x == a && x > b: 矛盾当 a <= b (e.g. x==4 && x>4)
            Ge(b) => a < b,  // x == a && x >= b: 矛盾当 a < b (e.g. x==4 && x>=5)
            Lt(b) => a >= b, // x == a && x < b: 矛盾当 a >= b (e.g. x==4 && x<4)
            Le(b) => a > b,  // x == a && x <= b: 矛盾当 a > b (e.g. x==4 && x<=3)
        },
        // existing: subject != a
        Ne(a) => match new {
            Eq(b) => b == a, // x != a && x == a
            _ => false,
        },
    }
}

/// 检查 `existing` 是否蕴含 `target`（在 existing 成立时 target 恒 true）。
///
/// 前提：调用方已确认 subject 相同。
pub(crate) fn bound_implies(existing: ConstraintBound, target: ConstraintBound) -> bool {
    use ConstraintBound::*;

    match existing {
        // x > a implies:
        Gt(a) => match target {
            Gt(b) => a >= b, // x > a → x > b when a >= b (e.g. x>5 → x>4)
            Ge(b) => a >= b, // x > a → x >= b when a >= b (e.g. x>5 → x>=5)
            Ne(b) => b <= a, // x > a → x != b when b <= a (e.g. x>5 → x!=5, x!=4)
            Lt(_) | Le(_) | Eq(_) => false,
        },
        // x >= a implies:
        Ge(a) => match target {
            Gt(b) => a > b,  // x >= a → x > b when a > b (e.g. x>=5 → x>4)
            Ge(b) => a >= b, // x >= a → x >= b when a >= b (e.g. x>=5 → x>=4)
            Ne(b) => b < a,  // x >= a → x != b when b < a (e.g. x>=5 → x!=4)
            Lt(_) | Le(_) | Eq(_) => false,
        },
        // x < a implies:
        Lt(a) => match target {
            Lt(b) => a <= b, // x < a → x < b when a <= b (e.g. x<4 → x<5)
            Le(b) => i128::from(a) <= i128::from(b) + 1, // x < a → x <= b when a <= b+1 (e.g. x<5 → x<=4)
            Ne(b) => b >= a, // x < a → x != b when b >= a (e.g. x<4 → x!=4, x!=5)
            Gt(_) | Ge(_) | Eq(_) => false,
        },
        // x <= a implies:
        Le(a) => match target {
            Lt(b) => a < b,  // x <= a → x < b when a < b (e.g. x<=4 → x<5)
            Le(b) => a <= b, // x <= a → x <= b when a <= b (e.g. x<=4 → x<=5)
            Ne(b) => b > a,  // x <= a → x != b when b > a (e.g. x<=4 → x!=5)
            Gt(_) | Ge(_) | Eq(_) => false,
        },
        // x == a implies:
        Eq(a) => match target {
            Eq(b) => a == b, // x == a → x == b when a == b
            Ne(b) => a != b, // x == a → x != b when a != b
            Gt(b) => a > b,  // x == a → x > b when a > b (e.g. x==5 → x>4)
            Ge(b) => a >= b, // x == a → x >= b when a >= b
            Lt(b) => a < b,  // x == a → x < b when a < b (e.g. x==3 → x<4)
            Le(b) => a <= b, // x == a → x <= b when a <= b
        },
        // x != a: very limited implication
        Ne(_) => false,
    }
}

/// 在 AND 上下文中检查 `expr` 是否因线性约束矛盾而恒 false。
///
/// 从 `context` 提取所有 `subject op literal` 约束，然后检查 `expr`
/// 是否与某个同 subject 约束矛盾。
///
/// 如果 `expr` 本身是 AND/OR 复合式，递归检查子式。
pub(crate) fn expr_contradicts_linear_context(expr: &Expr, context: &[Expr]) -> bool {
    // Try to extract a linear constraint from expr itself
    if let Some(target) = extract_linear_constraint(expr) {
        for ctx_item in context {
            if let Some(ctx_constraint) = extract_linear_constraint(ctx_item) {
                if ctx_constraint.subject == target.subject
                    && bounds_contradict(ctx_constraint.bound, target.bound)
                {
                    return true;
                }
            }
        }
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
    // Try to extract a linear constraint from expr itself
    if let Some(target) = extract_linear_constraint(expr) {
        for ctx_item in context {
            if let Some(ctx_constraint) = extract_linear_constraint(ctx_item) {
                if ctx_constraint.subject == target.subject
                    && bound_implies(ctx_constraint.bound, target.bound)
                {
                    return true;
                }
            }
        }
    }

    // The Boolean proof caller checks every leaf of a conjunction here.
    false
}
