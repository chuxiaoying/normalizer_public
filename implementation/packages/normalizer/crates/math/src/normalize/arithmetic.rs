//! 算术代数化简规则。
//!
//! 本模块放置 **非纯常量折叠** 的算术等价变换，例如
//! `(x - y) op 0 → x op y`。它不同于 `constant.rs`（两侧均为 Lit
//! 时直接求值），也不同于 `boolean.rs`（And/Or/Not 布尔代数）。
//!
//! ## 运算域
//!
//! 普通整数 Add/Sub/Mul 使用精确数学整数。源语言的位宽、回绕和窄化
//! 由显式 word 算子表达，不能混用其代数规则。普通差值与零的比较
//! 可比较两端，同一 subject 的精确偏移比较可消去共同项。

use crate::expr::{BinOpKind, Expr, ExprKind, Lit};

/// 算术代数化简入口。
///
/// 执行顺序：
/// 1. 先做幺元/零元化简（`simplify_identity`）——简化结构后再做后续规则。
/// 2. 再做 `(x - y) op 0 → x op y` 形变（`simplify_sub_compare_zero`）。
///
/// 幺元先行的理由：`x - 0` 先被规约为 `x`，避免后续 `(x - 0) op 0` 绕路；
/// simplify_once 的不动点迭代保证整体收敛。
pub(crate) fn simplify(expr: &Expr) -> Expr {
    if let Some(r) = simplify_identity(expr) {
        return r;
    }
    if let Some(r) = simplify_reflexive_equality(expr) {
        return r;
    }
    if let Some(r) = simplify_reference_comparison(expr) {
        return r;
    }
    if let Some(r) = simplify_same_subject_offset_comparison(expr) {
        return r;
    }
    if let Some(r) = simplify_sub_compare_zero(expr) {
        return r;
    }
    expr.clone()
}

/// 算术幺元/零元化简。
///
/// 规则清单：
/// - 加法幺元：`0 + x → x`、`x + 0 → x`
/// - 减法右零元：`x - 0 → x`（`0 - x` 不处理，需引入 UnaryOp::Neg）
/// - 乘法幺元：`1 * x → x`、`x * 1 → x`
/// - 乘法零元：`0 * x → 0`、`x * 0 → 0`
///
/// 这里仅化简纯值。原调用、状态写入和异常由独立的边界／完成信息
/// 保留；该规则不授权删除这些可观察行为，也不改变输入的运算域。
fn simplify_identity(expr: &Expr) -> Option<Expr> {
    let ExprKind::BinOp { op, left, right } = expr.kind() else {
        return None;
    };
    match op {
        BinOpKind::Add => {
            if is_lit_int_zero(left) {
                return Some(right.clone());
            }
            if is_lit_int_zero(right) {
                return Some(left.clone());
            }
        }
        BinOpKind::Sub => {
            if is_lit_int_zero(right) {
                return Some(left.clone());
            }
            if super::integer_ac::total_integer(expr) {
                if left == right {
                    return Some(crate::builder::lit_int(0));
                }
                if let Some(value) = remove_additive_term(left, right)
                    .or_else(|| combine_acquired_row_literals(left, right))
                {
                    return Some(value);
                }
            }
            // 0 - x 不处理：需引入 UnaryOp::Neg push-down，本模块不扩展
        }
        BinOpKind::Mul => {
            if is_lit_int_zero(left) || is_lit_int_zero(right) {
                return Some(crate::builder::lit_int(0));
            }
            if is_lit_int_one(left) {
                return Some(right.clone());
            }
            if is_lit_int_one(right) {
                return Some(left.clone());
            }
        }
        _ => {}
    }
    None
}

/// Remove one exact summand from a normalized sum without distributing any
/// multiplication. The caller has proved both totality and exact integers.
fn remove_additive_term(sum: &Expr, term: &Expr) -> Option<Expr> {
    let (BinOpKind::Add, left, right) = sum.as_binop()? else { return None; };
    if left == term { return Some(right.clone()); }
    if right == term { return Some(left.clone()); }
    if let Some(rest) = remove_additive_term(left, term) {
        return Some(crate::builder::add(rest, right.clone()));
    }
    remove_additive_term(right, term).map(|rest| crate::builder::add(left.clone(), rest))
}

// The exact row subject stays inside the same ordered query. Combine
// acquired literal offsets without lifting the row out of its scope.
fn combine_acquired_row_literals(left: &Expr, right: &Expr) -> Option<Expr> {
    let (BinOpKind::Add, base, left_literal) = left.as_binop()? else { return None; };
    let right = right.as_lit()?.as_integer()?;
    let left = left_literal.as_lit()?.as_integer()?;
    let mut has_acquired_row = false;
    base.walk(&mut |node| {
        has_acquired_row |= matches!(node.kind(), ExprKind::RowValue { acquired_integer: true, .. });
    });
    if !has_acquired_row { return None; }
    let rest = left - right;
    use num_traits::{Signed, Zero};
    if rest.is_zero() {
        Some(base.clone())
    } else if rest.is_negative() {
        Some(crate::builder::sub(base.clone(), crate::builder::lit_integer(-rest)))
    } else {
        Some(crate::builder::add(base.clone(), crate::builder::lit_integer(rest)))
    }
}

fn is_lit_int_one(expr: &Expr) -> bool {
    matches!(expr.kind(), ExprKind::Lit(Lit::Int(1)))
}

/// 自反等式/不等式化简。
///
/// 规则：
/// - `x == x -> true`
/// - `x != x -> false`
///
/// 这是纯数学/Java 引用自反性定律：同一个表达式值与自身比较恒等；当前 math
/// 不建模 NaN 浮点语义，且 Java reference self equality 也恒为 true。把该规则放在
/// math normalize 层，而不是 executor 的某个 JavaBinOp 局部 fast path，能让 inline
/// helper / CaseWhen / path condition 中已经形成的比较表达式统一受益，防止布尔条件
/// 在 caller 侧继续膨胀。
fn simplify_reflexive_equality(expr: &Expr) -> Option<Expr> {
    let ExprKind::BinOp { op, left, right } = expr.kind() else {
        return None;
    };
    if left != right {
        return None;
    }
    match op {
        BinOpKind::Eq | BinOpKind::Le | BinOpKind::Ge => Some(crate::builder::lit_bool(true)),
        BinOpKind::Ne | BinOpKind::Lt | BinOpKind::Gt => Some(crate::builder::lit_bool(false)),
        _ => None,
    }
}

/// Only fresh positive HeapRefs prove nonnull identity; negative HeapRefs
/// remain potentially aliased and nullable. Otherwise give null one direction.
fn simplify_reference_comparison(expr: &Expr) -> Option<Expr> {
    let (op, left, right) = expr.as_binop()?;
    if !matches!(op, BinOpKind::Eq | BinOpKind::Ne) {
        return None;
    }
    let positive = |value: &Expr| value.as_heap_ref().filter(|id| *id > 0);
    let distinct_positive =
        matches!((positive(left), positive(right)), (Some(a), Some(b)) if a != b);
    let positive_vs_null = (left.is_null() && positive(right).is_some())
        || (right.is_null() && positive(left).is_some());
    if distinct_positive || positive_vs_null {
        return Some(crate::builder::lit_bool(op == BinOpKind::Ne));
    }
    (left.is_null() && !right.is_null()).then(|| crate::builder::binop_public(op, right.clone(), left.clone()))
}

/// 同 subject 偏移量比较化简。
///
/// 精确整数加减保持平移不变；只在共同 subject 完全一致时消去。
/// 常量偏移同样使用任意精度，不能在提取阶段发生宿主溢出。
fn simplify_same_subject_offset_comparison(expr: &Expr) -> Option<Expr> {
    let ExprKind::BinOp { op, left, right } = expr.kind() else {
        return None;
    };
    if !matches!(
        op,
        BinOpKind::Lt | BinOpKind::Gt | BinOpKind::Le | BinOpKind::Ge
    ) {
        return None;
    }
    fn decompose(e: &Expr) -> (&Expr, num_bigint::BigInt) {
        if let ExprKind::BinOp { op, left, right } = e.kind() {
            if let Some(c) = right.as_lit().and_then(Lit::as_integer) {
                match op {
                    BinOpKind::Add => return (left, c),
                    BinOpKind::Sub => return (left, -c),
                    _ => {}
                }
            }
            if matches!(op, BinOpKind::Add) {
                if let Some(c) = left.as_lit().and_then(Lit::as_integer) {
                    return (right, c);
                }
            }
        }
        (e, 0.into())
    }
    let (base_l, off_l) = decompose(left);
    let (base_r, off_r) = decompose(right);
    if base_l != base_r {
        return None;
    }
    let result = match op {
        BinOpKind::Lt => off_l < off_r,
        BinOpKind::Gt => off_l > off_r,
        BinOpKind::Le => off_l <= off_r,
        BinOpKind::Ge => off_l >= off_r,
        _ => return None,
    };
    Some(crate::builder::lit_bool(result))
}

/// `(x - y) op 0 → x op y`。
///
/// 支持比较运算符：`> / < / >= / <= / == / !=`。
/// 对称形式先规范化方向：`0 < (x - y)` → `(x - y) > 0`。
fn simplify_sub_compare_zero(expr: &Expr) -> Option<Expr> {
    let ExprKind::BinOp { op, left, right } = expr.kind() else {
        return None;
    };
    let swapped = op.swapped_comparison()?;
    let (cmp_op, lhs) = if is_lit_int_zero(right) {
        (*op, left)
    } else if is_lit_int_zero(left) {
        (swapped, right)
    } else {
        return None;
    };
    let ExprKind::BinOp {
        op: BinOpKind::Sub,
        left: x,
        right: y,
    } = lhs.kind()
    else {
        return None;
    };

    Some(crate::builder::binop_public(cmp_op, x.clone(), y.clone()))
}

fn is_lit_int_zero(expr: &Expr) -> bool {
    matches!(expr.kind(), ExprKind::Lit(Lit::Int(0)))
}
