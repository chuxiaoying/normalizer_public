//! CaseWhen 布尔化规则（Step 2 of CaseWhen 扁平化系列）。
//!
//! 当 CaseWhen 所有分支 value 都是"布尔语义"表达式时，展开为 OR-of-AND 形式，
//! 让 CaseWhen 消失，交还给布尔代数/常量折叠链继续化简。
//!
//! ## 语义依据
//!
//! CaseWhen 按序匹配：`CASE WHEN c1 THEN v1 WHEN c2 THEN v2 ... WHEN true THEN vn END`
//! 作为布尔值等价于：
//! ```text
//! (c1 AND v1) OR (NOT c1 AND c2 AND v2) OR ... OR (NOT c1 AND ... AND NOT c_{n-1} AND vn)
//! ```
//!
//! 此等价性要求完整二值布尔分区；部分 CASE 未命中仍未定义，不能补成 false（与 short-circuit 求值一致：
//! 后续分支只在前面全 false 时被检查）。
//!
//! ## 判定"布尔语义"（保守）
//!
//! 只承认以下叶子结构为"布尔语义"：
//! - `Lit(Bool(_))`
//! - `BinOp` with op ∈ {Eq, Ne, Lt, Le, Gt, Ge, And, Or}
//! - `UnaryOp(Not, _)`
//! - 显式原始 Boolean 参数及注册表证明 Boolean 返回的调用
//! - 有默认分支的嵌套 CaseWhen，递归全分支 value 都是总 Boolean
//!
//! nullable boxed Boolean、无默认分支的部分 CASE 以及未证明类型的值不视为
//! 总 Boolean；不能仅凭条件位置、函数名或当前可观察分支猜测其类型。
//! 原始参数类型和函数注册表是证据来源，而不是重新反解 Java/库语义。
//!

//! ## 动机案例
//!
//! ChannelFacadeImpl.getConfig，经 Step 1 BinOp-on-CaseWhen 分发 + 常量折叠后：
//! ```text
//! CASE WHEN code==null THEN false WHEN true THEN "channel_"+code != null END
//! ```
//! 所有 value 已是布尔语义（Lit(Bool(false)) 和 BinOp(Ne,...)），本规律触发展开：
//! ```text
//! (code==null AND false) OR (NOT(code==null) AND ("channel_"+code != null))
//! ```
//! 后续常量/布尔代数化简：
//! ```text
//! false OR (NOT(code==null) AND ("channel_"+code != null))
//! → NOT(code==null) AND ("channel_"+code != null)
//! ```
//! 嵌套 CaseWhen 消失。
//!
//! ## 防爆炸
//!
//! 只展开"全分支布尔语义"的 CaseWhen，不做部分展开；不对非布尔 CaseWhen 触发。
//! 展开后交还给后续化简链处理（AND false → false，false OR x → x 等）。

use super::logical_chain::{rebuild_and, rebuild_or};
use crate::expr::{BinOpKind, Expr, ExprKind, Lit, UnaryOpKind};

pub(crate) fn simplify(expr: &Expr) -> Expr {
    // Only a complete Boolean value can become a total OR expression. A
    // partial CASE must remain undefined outside its selected domain.
    let Some((arms, fallback)) = expr.as_total_case_when() else {
        return expr.clone();
    };
    if !arms.iter().all(|(_, value)| is_boolean_expr(value)) || !is_boolean_expr(fallback) {
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

    let expanded = rebuild_or(&or_terms);

    // 防爆守门：CaseWhen 布尔化是为了让后续布尔代数继续收敛，
    // 不是为了把多分支 path/value 关系无条件摊平成 DNF。
    // 小型 boolean CaseWhen 仍允许展开，保留既有投影/覆盖证明能力；
    // 只有多分支 CaseWhen 展开会变大时，才保留分段形态，等待
    // path implication / CaseWhen first-match / 后续局部投影在上下文中化简。
    // 典型场景是 catch 变量由多条 throw path 组成，随后 equals/getter 把
    // code CaseWhen 当 boolean condition 消费；无条件 DNF 会把 throw-domain 摊爆。
    if arms.len() >= 3 && expanded.size() > expr.size() {
        return expr.clone();
    }

    expanded
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
