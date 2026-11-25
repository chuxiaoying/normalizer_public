//! BinOp-on-CaseWhen 分发规则。
//!
//! 归一化算子模板（同 FieldAccess/MethodCall on CaseWhen）：当 BinOp 的一侧
//! 是 CaseWhen 而另一侧是"简单"表达式时，把 BinOp 推进 CaseWhen 的 branches。
//!
//! ## 规则
//!
//! - `BinOp(op, CaseWhen{(c1,v1),...,(true,vn)}, simple)` →
//!   `CaseWhen{(c1, BinOp(op, v1, simple)), ..., (true, BinOp(op, vn, simple))}`
//!
//! - `BinOp(op, simple, CaseWhen{(c1,v1),...,(true,vn)})` →
//!   `CaseWhen{(c1, BinOp(op, simple, v1)), ..., (true, BinOp(op, simple, vn))}`
//!
//! ## 防爆炸
//!
//! 另一侧必须是 "simple" 表达式（Lit / SymVar / HeapRef），避免 CaseWhen × CaseWhen
//! 笛卡尔爆炸。若两侧都是 CaseWhen，本规则不触发，留给上游设计者处理。
//!
//! ## 语义依据
//!
//! BinOp 是纯函数语义（对比较/算术/布尔运算而言），`op(CaseWhen, x)` 在每条分支
//! 上独立求值结果等价于原表达式——这是 CaseWhen 作为"值级条件分支"的自然分发律。
//!
//! ## 动机案例
//!
//! ChannelFacadeImpl.getConfig（嵌套 CaseWhen）：
//! ```text
//! CASE WHEN (CASE WHEN c==null THEN null WHEN true THEN "ch_"+c END) != null
//!      THEN ...
//! ```
//! 分发后：
//! ```text
//! CASE WHEN (CASE WHEN c==null THEN (null!=null) WHEN true THEN ("ch_"+c!=null) END)
//!      THEN ...
//! ```
//! 配合 constant 折叠 `null!=null → false`，为后续 dead branch 消除创造条件。

use crate::expr::{Expr, ExprKind};

pub(crate) fn simplify(expr: &Expr) -> Expr {
    let ExprKind::BinOp { op, left, right } = expr.kind() else {
        return expr.clone();
    };

    let left_is_cw = matches!(left.kind(), ExprKind::CaseWhen { .. });
    let right_is_cw = matches!(right.kind(), ExprKind::CaseWhen { .. });

    // 两侧都是 CaseWhen：不触发，防止笛卡尔爆炸。
    if left_is_cw && right_is_cw {
        return expr.clone();
    }

    // When a branch selected the subtrahend itself, the corresponding
    // difference is exactly zero. Distribute only if this actually shrinks
    // the tree; never expand two CASE expressions into a cross product.
    if *op == crate::BinOpKind::Sub && super::integer_ac::total_integer(right) {
        if let Some(branches) = left.as_case_when() {
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

    // 左侧 CaseWhen + 右侧 simple
    if left_is_cw && is_simple(right) && should_distribute(left, right) {
        if let ExprKind::CaseWhen { branches } = left.kind() {
            let new_branches: Vec<(Expr, Expr)> = branches
                .iter()
                .map(|(c, v)| {
                    let new_v = Expr::new(ExprKind::BinOp {
                        op: *op,
                        left: v.clone(),
                        right: right.clone(),
                    });
                    (c.clone(), new_v)
                })
                .collect();
            return Expr::new(ExprKind::CaseWhen {
                branches: new_branches,
            });
        }
    }

    // 右侧 CaseWhen + 左侧 simple
    if right_is_cw && is_simple(left) && should_distribute(right, left) {
        if let ExprKind::CaseWhen { branches } = right.kind() {
            let new_branches: Vec<(Expr, Expr)> = branches
                .iter()
                .map(|(c, v)| {
                    let new_v = Expr::new(ExprKind::BinOp {
                        op: *op,
                        left: left.clone(),
                        right: v.clone(),
                    });
                    (c.clone(), new_v)
                })
                .collect();
            return Expr::new(ExprKind::CaseWhen {
                branches: new_branches,
            });
        }
    }

    expr.clone()
}

/// "简单"表达式判定：Lit / SymVar / HeapRef。
///
/// 排除：CaseWhen（防爆炸）、BinOp/UnaryOp/Call/ArrayLit（避免把复杂表达式复制 N 份）。
fn is_simple(expr: &Expr) -> bool {
    matches!(
        expr.kind(),
        ExprKind::Lit(_) | ExprKind::SymVar { .. } | ExprKind::HeapRef(_)
    )
}

fn should_distribute(case_when: &Expr, simple_side: &Expr) -> bool {
    let ExprKind::CaseWhen { branches } = case_when.kind() else {
        return true;
    };
    if branches.len() <= 4 && (case_when.size() > 2048 || case_when.depth() > 48) {
        return false;
    }
    if simple_side.size() > 64
        || simple_side.depth() > 16
        || matches!(simple_side.kind(), ExprKind::CaseWhen { .. })
    {
        return false;
    }
    true
}
