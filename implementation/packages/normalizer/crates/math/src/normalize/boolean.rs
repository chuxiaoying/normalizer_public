//! 布尔代数规则。
//!
//! 二元规则：
//! - 幺元：and(x, true) = x, or(x, false) = x
//! - 吸收元：and(x, false) = false, or(x, true) = true
//! - 双重否定：not(not(x)) = x
//! - 互补律：and(x, not(x)) = false, or(x, not(x)) = true
//! - 幂等：and(x, x) = x, or(x, x) = x
//!
//! 多元规则（flatten 后处理）：
//! - 多元幂等去重：a && b && a && c → a && b && c
//! - OR disjunct 规范排序：交换律下统一输出顺序，保证归一 equals 稳定
//! - 多元常量消除：and 中过滤 true，or 中过滤 false
//! - 多元吸收元：and 遇到 false → false，or 遇到 true → true
//! - 多元互补检测：conjuncts 包含 x 和 NOT(x) → false；disjuncts 类似 → true
//!
//! 用户明令（20260428180545）："等价逻辑下越简单越正确"。多元 flatten+dedupe
//! 是归一化 equals 的基础前提——语法结构 `((A && A) && B) && A` 通过二元 `x && x`
//! 规则无法完全 dedupe，必须 flatten 到扁平 conjuncts 列表后统一去重。

use super::logical_chain::{flatten_and, flatten_or, rebuild_and, rebuild_or};
use crate::builder;
use crate::expr::{BinOpKind, Expr, ExprKind, Lit, RelationAggregateKind, UnaryOpKind};
use crate::symvar::SymVarOrigin;
use implication::*;

pub(crate) fn simplify(expr: &Expr, ctx: &dyn crate::sort_context::SortContext) -> Expr {
    match expr.kind() {
        ExprKind::BinOp {
            op: BinOpKind::And, ..
        } => simplify_and_multi(expr, ctx).unwrap_or_else(|| expr.clone()),
        ExprKind::BinOp {
            op: BinOpKind::Or, ..
        } => simplify_or_multi(expr, ctx).unwrap_or_else(|| expr.clone()),
        ExprKind::BinOp { op, left, right } if matches!(op, BinOpKind::Eq | BinOpKind::Ne) => {
            if let Some(value) = boolean_literal_equality(*op, left, right) {
                return value;
            }
            // Input reference identities may alias. Canonicalize only the
            // symmetric pair, never infer inequality from different IDs.
            if let (Some(a), Some(z)) = (left.as_heap_ref(), right.as_heap_ref()) {
                if a > z {
                    return builder::binop_public(*op, right.clone(), left.clone());
                }
            }
            expr.clone()
        }
        ExprKind::UnaryOp {
            op: UnaryOpKind::Not,
            operand,
        } => simplify_not(operand).unwrap_or_else(|| expr.clone()),
        _ => expr.clone(),
    }
}

/// Unlike boxed Boolean and partial CASE, these values have exactly the two
/// Boolean values on their defined input domain. Never equate null with false.
pub(crate) fn total_boolean_value(expr: &Expr) -> bool {
    match expr.kind() {
        ExprKind::Lit(Lit::Bool(_)) => true,
        ExprKind::SymVar {
            origin:
                SymVarOrigin::Parameter { primitive_type, .. }
                | SymVarOrigin::External { primitive_type, .. }
                | SymVarOrigin::Temporal { primitive_type, .. },
        } => primitive_type == "boolean",
        ExprKind::BinOp {
            op: BinOpKind::And | BinOpKind::Or,
            left,
            right,
        } => total_boolean_value(left) && total_boolean_value(right),
        ExprKind::BinOp { op, .. } => matches!(
            op,
            BinOpKind::Eq
                | BinOpKind::Ne
                | BinOpKind::Lt
                | BinOpKind::Le
                | BinOpKind::Gt
                | BinOpKind::Ge
        ),
        ExprKind::UnaryOp {
            op: UnaryOpKind::Not,
            operand,
        } => total_boolean_value(operand),
        ExprKind::RelationAggregate {
            kind: RelationAggregateKind::Exists,
            ..
        } => true,
        ExprKind::CaseWhen { branches } => {
            builder::conditions_cover_all(
                &branches.iter().map(|(c, _)| c.clone()).collect::<Vec<_>>(),
            ) && branches.iter().all(|(_, v)| total_boolean_value(v))
        }
        _ => false,
    }
}
fn boolean_literal_equality(op: BinOpKind, left: &Expr, right: &Expr) -> Option<Expr> {
    let (value, literal) = if let Some(v) = right.as_bool() {
        (left, v)
    } else if let Some(v) = left.as_bool() {
        (right, v)
    } else {
        return None;
    };
    if !total_boolean_value(value) {
        return None;
    }
    Some(if literal == (op == BinOpKind::Eq) {
        value.clone()
    } else {
        builder::not(value.clone())
    })
}

/// 分配律化简：`A1 && A2 && ... && OR(B1, B2, ...)` → `OR(A1&A2&...&B1, A1&A2&...&B2, ...)`
///
/// 仅在启发式认为"分配后能消元"时触发，避免无谓膨胀：
/// - context（OR 外的合取因子）非空
/// - 至少一个 OR 分支的合取中，存在与 context 某因子"相等或互补"的子句
///
/// 合法性依赖布尔代数分配律；返回的新 OR 会被 fixpoint 调用 normalize 继续化简，
/// 所以本函数只负责结构变换，不做具体 AND 化简。
fn simplify_or_under_and_context(or_expr: &Expr, context: &[Expr]) -> Option<Expr> {
    let disjuncts = flatten_or(or_expr);
    if disjuncts.len() < 2 || context.is_empty() {
        return None;
    }

    let mut changed = false;
    let mut kept: Vec<Expr> = Vec::with_capacity(disjuncts.len());

    for disjunct in disjuncts {
        if expr_is_true_under_and_context(&disjunct, context) {
            return Some(builder::lit_bool(true));
        }
        if expr_is_false_under_and_context(&disjunct, context) {
            changed = true;
            continue;
        }

        let conjuncts = flatten_and(&disjunct);
        if conjuncts.len() > 1 {
            // The whole disjunct is already known not to be entirely true
            // and to contain no contradicted conjunct under this context.
            let mut reduced: Vec<Expr> = Vec::with_capacity(conjuncts.len());
            for conjunct in conjuncts {
                if expr_is_true_under_and_context(&conjunct, context) {
                    changed = true;
                    continue;
                }
                reduced.push(conjunct);
            }
            let reduced_expr = rebuild_and(&reduced);
            if reduced_expr != disjunct {
                changed = true;
            }
            kept.push(reduced_expr);
        } else {
            kept.push(disjunct);
        }
    }

    if !changed {
        return None;
    }
    if kept.is_empty() {
        return Some(builder::lit_bool(false));
    }
    if kept.len() == 1 {
        return Some(kept.into_iter().next().unwrap());
    }
    Some(rebuild_or(&kept))
}

fn expr_is_true_under_and_context(expr: &Expr, context: &[Expr]) -> bool {
    if matches!(expr.kind(), ExprKind::Lit(Lit::Bool(true))) {
        return true;
    }
    if context.iter().any(|c| {
        c == expr
            || (!profile_pair_can_skip_proof_check(c, expr)
                && (relation_count_nonzero_implies_exists(c, expr)
                    || literal_equality_implies_literal_inequality(c, expr)
                    || or_all_branches_imply(c, expr)))
    }) {
        return true;
    }
    // 线性约束蕴含检测：x > 4 下 x > 3 恒 true
    if super::constraint::expr_implied_by_linear_context(expr, context) {
        return true;
    }
    let conjuncts = flatten_and(expr);
    if conjuncts.len() > 1 {
        return conjuncts
            .iter()
            .all(|c| expr_is_true_under_and_context(c, context));
    }

    let disjuncts = flatten_or(expr);
    disjuncts.len() > 1
        && disjuncts
            .iter()
            .any(|d| expr_is_true_under_and_context(d, context))
}

fn expr_is_false_under_and_context(expr: &Expr, context: &[Expr]) -> bool {
    if matches!(expr.kind(), ExprKind::Lit(Lit::Bool(false))) {
        return true;
    }
    if context.iter().any(|c| {
        !profile_pair_can_skip_proof_check(c, expr)
            && (is_complement(c, expr) || relation_cardinality_contradicts(c, expr))
    }) {
        return true;
    }
    // 线性约束矛盾检测：x > 4 下 x < 4 恒 false
    if super::constraint::expr_contradicts_linear_context(expr, context) {
        return true;
    }
    // Primitive predicate null-behavior contradiction:
    //   (x == null) && !isBlank(x) -> false, when isBlank(null)=true
    //   (x == null) && isNotBlank(x) -> false, when isNotBlank(null)=false
    //
    // `!isBlank(x)` / `isNotBlank(x)` already proves `x != null` by the
    // registered Primitive Function null_behavior.  This local proof lets OR
    // branches such as `(x == null || y == null)` collapse under an AND context
    // that has already established both predicates, instead of leaking
    // impossible NPE/throw domains from source-visible helpers.
    if let Some(null_subject) = null_comparison_subject(expr, BinOpKind::Eq) {
        if context.iter().any(|c| {
            predicate_implies_nonnull_subject(c)
                .as_ref()
                .is_some_and(|subject| subject == &null_subject)
        }) {
            return true;
        }
    }
    if let ExprKind::UnaryOp {
        op: UnaryOpKind::Not,
        operand,
    } = expr.kind()
    {
        if expr_is_true_under_and_context(operand, context) {
            return true;
        }
    }

    let conjuncts = flatten_and(expr);
    if conjuncts.len() > 1 {
        return conjuncts
            .iter()
            .any(|c| expr_is_false_under_and_context(c, context));
    }

    let disjuncts = flatten_or(expr);
    disjuncts.len() > 1
        && disjuncts
            .iter()
            .all(|d| expr_is_false_under_and_context(d, context))
}

// The caller has at least two surviving disjuncts.
fn factor_common_and_conjuncts_under_or(disjuncts: &[Expr]) -> Option<Expr> {
    // Every input produces at least one term, including an atomic input.
    let all_conjs: Vec<Vec<Expr>> = disjuncts.iter().map(flatten_and).collect();

    let mut common: Vec<Expr> = Vec::new();
    for candidate in &all_conjs[0] {
        if all_conjs[1..]
            .iter()
            .all(|conjs| conjs.iter().any(|expr| expr == candidate))
            && !common.iter().any(|expr| expr == candidate)
        {
            common.push(candidate.clone());
        }
    }
    if common.is_empty() {
        return None;
    }

    let remainders: Vec<Expr> = all_conjs.into_iter().map(|conjs| {
        let rest: Vec<Expr> = conjs.into_iter()
            .filter(|expr| !common.contains(expr))
            .collect();
        rebuild_and(&rest)
    }).collect();
    let common_expr = rebuild_and(&common);
    let remainder_expr = rebuild_or(&remainders);
    Some(builder::and(common_expr, remainder_expr))
}

fn factor_common_or_disjunct_under_and(conjuncts: &[Expr]) -> Option<Expr> {
    if conjuncts.len() < 2 {
        return None;
    }

    for i in 0..conjuncts.len() {
        let left_or = flatten_or(&conjuncts[i]);
        if left_or.len() < 2 {
            continue;
        }
        for j in (i + 1)..conjuncts.len() {
            let right_or = flatten_or(&conjuncts[j]);
            if right_or.len() < 2 {
                continue;
            }

            let mut common = Vec::new();
            let mut left_rest = Vec::new();
            for left in &left_or {
                if right_or.iter().any(|right| right == left) {
                    if !common.iter().any(|expr| expr == left) {
                        common.push(left.clone());
                    }
                } else {
                    left_rest.push(left.clone());
                }
            }
            if common.is_empty() || left_rest.is_empty() {
                continue;
            }

            let right_rest: Vec<Expr> = right_or
                .iter()
                .filter(|right| !common.iter().any(|common| common == *right))
                .cloned()
                .collect();
            if right_rest.is_empty() {
                continue;
            }

            // Guard against blind distribution.  This rewrite is most useful
            // when the remainders can immediately simplify, e.g.
            // `(C || X || Y) && (C || !Y) -> C || ((X || Y) && !Y) -> C || (X && !Y)`.
            if !left_rest
                .iter()
                .any(|left| right_rest.iter().any(|right| is_complement(left, right)))
            {
                continue;
            }

            let common_expr = rebuild_or(&common);
            let left_expr = rebuild_or(&left_rest);
            let right_expr = rebuild_or(&right_rest);
            let replacement = builder::or(common_expr, builder::and(left_expr, right_expr));

            let mut next = Vec::with_capacity(conjuncts.len() - 1);
            for (idx, conjunct) in conjuncts.iter().enumerate() {
                if idx != i && idx != j {
                    next.push(conjunct.clone());
                }
            }
            next.push(replacement);
            return Some(rebuild_and(&next));
        }
    }

    None
}

// =============================================================================
// 多元 AND 化简
// =============================================================================

fn simplify_and_multi(expr: &Expr, ctx: &dyn crate::sort_context::SortContext) -> Option<Expr> {
    let flat = flatten_and(expr);
    if let Some(solved) = super::integer_equations::solve(&flat) {
        return Some(solved);
    }
    let mut out: Vec<Expr> = Vec::with_capacity(flat.len());
    for item in &flat {

        // 吸收元：遇到 false 立即返回 false
        if let ExprKind::Lit(Lit::Bool(false)) = item.kind() {
            return Some(builder::lit_bool(false));
        }
        // 幺元：跳过 true
        if let ExprKind::Lit(Lit::Bool(true)) = item.kind() {
            continue;
        }
        // 幂等去重：已存在则跳过
        if out.iter().any(|e| e == item) {
            continue;
        }
        // 互补检测：若已有 NOT(item) 或 item 是已有项的 NOT，整体化简为 false
        if out
            .iter()
            .any(|e| !profile_pair_can_skip_proof_check(e, item) && is_complement(e, item))
        {
            return Some(builder::lit_bool(false));
        }
        // A row alias does not change whether the same relation is empty.
        // The same complement used by OR coverage also refutes conjunctions.
        if out.iter().any(|existing| {
            relation_exists_or_not_exists_same_relation(existing, item)
                || relation_exists_or_not_exists_same_relation(item, existing)
        }) {
            return Some(builder::lit_bool(false));
        }
        // Relation cardinality contradiction:
        //   Exists(R) && Count(R) == 0 -> false
        //
        // This is Relation Calculus cardinality algebra.  It is needed before
        // executor bounds checks can erase impossible exception domains under a
        // non-empty path guard; do not encode this as a List.get special case.
        if out.iter().any(|existing| {
            !profile_pair_can_skip_proof_check(existing, item)
                && relation_cardinality_contradicts(existing, item)
        }) {
            return Some(builder::lit_bool(false));
        }
        // 上下文矛盾：例如 `!Exists(R) && Exists(Select(P,R))`。
        //
        // `Exists(Select(P,R)) => Exists(R)` 是 Relation Calculus 的通用
        // cardinality 蕴含；这里仍只使用当前 boolean 层的窄 proof helper，
        // 不调用 path_implies/normalize，避免递归自咬。
        if expr_is_false_under_and_context(item, &out)
            || out.iter().any(|existing| {
                expr_is_false_under_and_context(existing, std::slice::from_ref(item))
            })
        {
            return Some(builder::lit_bool(false));
        }
        // Ordered comparison contradiction: (x > 0) && (x <= 0) → false.
        // This quick local check keeps early contradiction behavior while the
        // full same-subject interval system below computes the canonical
        // solution constraints for all ordered comparisons in the conjunction.
        if out.iter().any(|e| ordered_comparisons_contradict(e, item)) {
            return Some(builder::lit_bool(false));
        }

        // A concrete literal and a positive allocation identity each form
        // their own closed equality class. Do not cross the domains.
        let mut skip_item = false;
        for domain in [EqualityAtomDomain::Literal, EqualityAtomDomain::Allocated] {
            let Some((op, subject, atom)) = equality_atom(item, domain) else { continue; };
            if op == BinOpKind::Eq {
                let mut contradiction = false;
                out.retain(|existing| {
                    let Some((op, other_subject, other)) = equality_atom(existing, domain) else { return true; };
                    if other_subject != subject { return true; }
                    if op == BinOpKind::Eq {
                        contradiction |= atom != other;
                        true
                    } else if atom == other {
                        contradiction = true;
                        true
                    } else {
                        false
                    }
                });
                if contradiction { return Some(builder::lit_bool(false)); }
                if matches!(domain, EqualityAtomDomain::Allocated) {
                    out.retain(|existing| {
                        null_comparison_subject(existing, BinOpKind::Ne)
                            .as_ref().is_none_or(|candidate| candidate != &subject)
                    });
                }
            } else {
                for existing in &out {
                    let Some((existing_op, existing_subject, other)) = equality_atom(existing, domain) else { continue; };
                    if existing_op != BinOpKind::Eq || existing_subject != subject { continue; }
                    if atom == other { return Some(builder::lit_bool(false)); }
                    skip_item = true;
                    break;
                }
            }
            if skip_item { break; }
        }
        if skip_item { continue; }

        // Primitive predicate null-behavior absorption:
        //   x != null && !isBlank(x)  ->  !isBlank(x), when isBlank(null)=true
        //   x != null && isNotBlank(x) ->  isNotBlank(x), when isNotBlank(null)=false
        //
        // This is a math-level consequence of the registered Primitive Function
        // null_behavior.  The predicate itself already proves non-nullness of
        // its subject; keeping the Java implementation guard inflates large
        // business conditions such as route-channel response checks.
        if let Some(non_null_subject) = null_comparison_subject(item, BinOpKind::Ne) {
            if out.iter().any(|existing| {
                predicate_implies_nonnull_subject(existing)
                    .as_ref()
                    .is_some_and(|subject| subject == &non_null_subject)
                    || heap_ref_eq_ne_comparison(existing).is_some_and(|(op, subj, id)| {
                        op == BinOpKind::Eq && id > 0 && subj == non_null_subject
                    })
            }) {
                continue;
            }
        }
        if let Some(predicate_subject) = predicate_implies_nonnull_subject(item) {
            out.retain(|existing| {
                null_comparison_subject(existing, BinOpKind::Ne)
                    .as_ref()
                    .is_none_or(|subject| subject != &predicate_subject)
            });
        }

        out.push(item.clone());
    }

    // Narrow RCL cardinality absorption:
    //   Count(R) > 0  <=>  Exists(R)
    //
    // In AND contexts keep the stronger/arithmetic Count fact and remove the
    // duplicate Exists observer for the same relation. A proven empty input
    // likewise absorbs emptiness of a strict filtered subset. This is relation
    // cardinality semantics, not broad peer implication.
    let absorbed_by_cardinality: Vec<bool> = (0..out.len())
        .map(|i| {
            (0..out.len())
                .any(|j| i != j && (relation_count_nonzero_implies_exists(&out[j], &out[i])
                    || empty_input_implies_empty_selection(&out[j], &out[i])))
        })
        .collect();
    if absorbed_by_cardinality.iter().any(|absorbed| *absorbed) {
        out = out
            .into_iter()
            .zip(absorbed_by_cardinality.iter())
            .filter_map(|(expr, absorbed)| if *absorbed { None } else { Some(expr) })
            .collect();
        // Count witnesses survive. For empty selections, each support edge
        // points from a strict ancestor to its descendant, so a root survives.
        return Some(rebuild_and(&out));
    }

    // Solve same-subject ordered comparison constraints as an interval system:
    //
    //   x > 0 && x > 1   -> x > 1
    //   x > 0 && x <= 0  -> false
    //   x >= 0 && x <= 2 -> x >= 0 && x <= 2
    //
    // This is the conjunction solution set, not a formatter rule.  We only
    // handle literal integer bounds here; non-literal arithmetic remains
    // untouched until a richer solver exists.
    if let Some(solved) = solve_ordered_comparison_conjunction(&out) {
        if solved.len() == 1 {
            return Some(solved.into_iter().next().unwrap());
        }
        // The interval solver produces lower/upper bounds in semantic order.
        // Peer Boolean terms still use the one shared canonical ordering;
        // otherwise interval reconstruction and AND sorting oscillate forever.
        let solved = sort_commutative(solved, ctx);
        let canonical = rebuild_and(&solved);
        if canonical != *expr {
            return Some(canonical);
        }
    }

    // An OR conjunct can absorb an inequality only if every alternative
    // implies it. Literal equality and certified fresh HeapRef identity are
    // distinct evidence, not interchangeable with displayed names.
    {
        let or_implies_absorbed: Vec<bool> = (0..out.len())
            .map(|i| {
                if matches!(
                    out[i].kind(),
                    ExprKind::BinOp {
                        op: BinOpKind::Or,
                        ..
                    }
                ) {
                    return false; // don't remove the OR itself
                }
                (0..out.len()).any(|j| {
                    i != j
                        && matches!(
                            out[j].kind(),
                            ExprKind::BinOp {
                                op: BinOpKind::Or,
                                ..
                            }
                        )
                        && or_all_branches_imply(&out[j], &out[i])
                })
            })
            .collect();
        if or_implies_absorbed.iter().any(|a| *a) {
            // The witnessing OR term is never marked for removal.
            let remaining: Vec<_> = out.into_iter().zip(or_implies_absorbed)
                .filter_map(|(expr, absorbed)| (!absorbed).then_some(expr))
                .collect();
            return Some(rebuild_and(&remaining));
        }
    }

    // Do not perform broad peer-implication absorption for arbitrary AND
    // conjuncts here.  Rules like `NOT(xs.isEmpty()) => xs.length > 0` are
    // useful inside a local guard, but applying them between unrelated path
    // facts from different boundary calls can erase the side-specific
    // non-empty facts that distinguish CaseWhen domains.  Keep the narrower
    // OR-under-context rewrite below, which only rewrites the OR factor being
    // simplified under the explicit surrounding AND context.

    if let Some(factored) = factor_common_or_disjunct_under_and(&out) {
        return Some(factored);
    }

    // Simplify OR factors under the surrounding AND context:
    //   C && (C || X)  -> C
    //   C && (!C || X) -> C && X
    //
    // This is pure boolean algebra.  It avoids exploding early-return path
    // guards by removing facts already known from the current continuation
    // context before any expensive distribution.
    let mut context_simplified: Vec<Expr> = Vec::with_capacity(out.len());
    let mut context_changed = false;
    for (idx, item) in out.iter().enumerate() {
        if matches!(
            item.kind(),
            ExprKind::BinOp {
                op: BinOpKind::Or,
                ..
            }
        ) {
            let context: Vec<Expr> = out
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != idx)
                .map(|(_, e)| e.clone())
                .collect();
            if let Some(simplified) = simplify_or_under_and_context(item, &context) {
                context_changed = true;
                if let ExprKind::Lit(Lit::Bool(false)) = simplified.kind() {
                    return Some(builder::lit_bool(false));
                }
                if !matches!(simplified.kind(), ExprKind::Lit(Lit::Bool(true))) {
                    context_simplified.push(simplified);
                }
                continue;
            }
        }
        context_simplified.push(item.clone());
    }
    if context_changed {
        return Some(rebuild_and(&context_simplified));
    }

    // 全部过滤掉 → true（空合取 = 恒真）
    if out.is_empty() {
        return Some(builder::lit_bool(true));
    }
    // 单项 → 该项
    if out.len() == 1 {
        return Some(out.into_iter().next().unwrap());
    }
    // AND 和 OR 共用统一正则规范的结构排序键。
    let out = sort_commutative(out, ctx);
    // Sorting the leaves is not enough: differently associated trees can
    // have identical flat order. Compare the complete canonical left fold.
    let canonical = rebuild_and(&out);
    (canonical != *expr).then_some(canonical)
}

// =============================================================================
// 多元 OR 化简
// =============================================================================

fn simplify_or_multi(expr: &Expr, ctx: &dyn crate::sort_context::SortContext) -> Option<Expr> {
    let flat = flatten_or(expr);
    let mut out: Vec<Expr> = Vec::with_capacity(flat.len());
    for item in &flat {
        // 吸收元：遇到 true 立即返回 true
        if let ExprKind::Lit(Lit::Bool(true)) = item.kind() {
            return Some(builder::lit_bool(true));
        }
        // 幺元：跳过 false
        if let ExprKind::Lit(Lit::Bool(false)) = item.kind() {
            continue;
        }
        // 幂等去重
        if out.iter().any(|e| e == item) {
            continue;
        }
        // 互补检测：x || NOT(x) → true.
        if out.iter().any(|e| !profile_pair_can_skip_proof_check(e, item) && is_complement(e, item)) {
            return Some(builder::lit_bool(true));
        }
        // A single pure value cannot equal two distinct exact integers.
        // This dual of equality contradiction also makes path implication
        // prove x=5 => x!=0 without requiring a particular term spelling.
        if let Some((BinOpKind::Ne, subject, Lit::Int(value))) = literal_eq_ne_comparison(item) {
            if out.iter().any(|other| {
                matches!(literal_eq_ne_comparison(other),
                Some((BinOpKind::Ne, ref other_subject, Lit::Int(other_value)))
                    if *other_subject == subject && other_value != value)
            }) {
                return Some(builder::lit_bool(true));
            }
        }
        // Primitive predicate null-behavior absorption:
        //   x == null || isBlank(x)   ->  isBlank(x), when isBlank(null)=true
        //   x == null || !isPresent(x)->  !isPresent(x), when isPresent(null)=false
        //
        // This is a math-level consequence of registered Primitive Function
        // null_behavior.  The predicate already covers the null case, so the
        // explicit Java implementation guard is redundant.  Do not encode
        // function names here; only consume FuncSemantics.
        if let Some(null_subject) = null_comparison_subject(item, BinOpKind::Eq) {
            if out.iter().any(|existing| {
                predicate_covers_null_subject(existing)
                    .as_ref()
                    .is_some_and(|subject| subject == &null_subject)
            }) {
                continue;
            }
        }
        if let Some(predicate_subject) = predicate_covers_null_subject(item) {
            out.retain(|existing| {
                null_comparison_subject(existing, BinOpKind::Eq)
                    .as_ref()
                    .is_none_or(|subject| subject != &predicate_subject)
            });
            if out.iter().any(|existing| {
                null_comparison_subject(existing, BinOpKind::Ne)
                    .as_ref()
                    .is_some_and(|subject| subject == &predicate_subject)
            }) {
                return Some(builder::lit_bool(true));
            }
        }
        if let Some(non_null_subject) = null_comparison_subject(item, BinOpKind::Ne) {
            if out.iter().any(|existing| {
                predicate_covers_null_subject(existing)
                    .as_ref()
                    .is_some_and(|subject| subject == &non_null_subject)
            }) {
                return Some(builder::lit_bool(true));
            }
        }
        out.push(item.clone());
    }

    if out.is_empty() {
        return Some(builder::lit_bool(false));
    }
    if out.len() == 1 {
        return Some(out.into_iter().next().unwrap());
    }

    if let Some(factored) = factor_common_and_conjuncts_under_or(&out) {
        return Some(factored);
    }

    // Two products sharing a factor can merge when their remainders
    // are complementary: (C && X) || (C && !X) == C. One pass handles
    // atomic remainders and compound De Morgan pairs alike.
    out = common_factor_complement_reduce(&out);
    // 逆吸收律 resolution absorption：A || (!A ∧ X) → A || X
    // 对每个 anchor disjunct d_i，从其它 disjunct d_j 的 conjuncts 中删除与 d_i 互补的因子
    out = resolution_absorb(&out);
    out = comparison_or_absorb(&out);
    if super::integer_ranges::exhaustive_union(&out) {
        return Some(builder::lit_bool(true));
    }
    if out.len() == 1 {
        return Some(out.into_iter().next().unwrap());
    }
    // Resolution can emit true directly; every ordinary factor already
    // went through the first pass and is handled by shared OR absorption.
    if out.iter().any(|item| item.as_bool() == Some(true)) {
        return Some(builder::lit_bool(true));
    }

    let out = sort_commutative(out, ctx);

    // 吸收律：A || (A ∧ X) = A
    // 若 OR 项 out[j] 的 AND 合取集合是 OR 项 out[i] 的 AND 合取集合的（真）子集，
    // 则 out[i] = out[j] ∧ 剩余，被 out[j] 蕴含，out[i] 可删。
    //
    // 注意：相等情形已由前面的 dedupe 处理（Expr 结构完全相同才 dedupe）；
    // 这里覆盖"合取集合相等但 Expr 结构不同"的情形（左折叠 vs 右折叠），
    // 以 i > j 为 tiebreaker 避免互相吸收。
    let all_conjs: Vec<Vec<Expr>> = out.iter().map(flatten_and).collect();
    let absorbed: Vec<bool> = (0..out.len())
        .map(|i| {
            let conjs_i = &all_conjs[i];
            (0..out.len()).any(|j| {
                if i == j {
                    return false;
                }
                let conjs_j = &all_conjs[j];
                if conjs_j.len() > conjs_i.len() {
                    return false;
                }
                // The context proof already includes literal equality,
                // so the exact subset case needs no second search.
                conjunction_implies(conjs_i, conjs_j)
                    && (!conjunction_implies(conjs_j, conjs_i) || i > j)
            })
        })
        .collect();
    // Removing terms does not change the remaining canonical order.
    let out: Vec<Expr> = out.into_iter().zip(absorbed)
        .filter_map(|(expr, absorbed)| (!absorbed).then_some(expr))
        .collect();
    let canonical = rebuild_or(&out);
    (canonical != *expr).then_some(canonical)
}

mod implication;

fn sort_commutative(
    mut factors: Vec<Expr>,
    ctx: &dyn crate::sort_context::SortContext,
) -> Vec<Expr> {
    // Cache each immutable expression's key once in the current context.
    factors.sort_by_cached_key(|factor| crate::canonical_order_key(factor, ctx));
    factors
}

// =============================================================================
// NOT 化简
// =============================================================================

fn simplify_not(operand: &Expr) -> Option<Expr> {
    // NOT over a boolean CaseWhen is a boolean CaseWhen with negated branch
    // payloads. This is deliberately restricted to all-boolean payloads:
    // non-boolean/null CaseWhen values may carry Java null/unboxing semantics
    // and must not be treated as false.
    if let ExprKind::CaseWhen { branches } = operand.kind() {
        if branches
            .iter()
            .all(|(_, value)| super::boolean_casewhen::is_boolean_expr(value))
        {
            return Some(Expr::new(ExprKind::CaseWhen {
                branches: branches
                    .iter()
                    .map(|(cond, value)| (cond.clone(), builder::not(value.clone())))
                    .collect(),
            }));
        }
    }

    // not(not(x)) => x
    if let ExprKind::UnaryOp {
        op: UnaryOpKind::Not,
        operand: inner,
    } = operand.kind()
    {
        return Some(inner.clone());
    }
    // 比较取反：NOT(x == y) → x != y，NOT(a < b) → a >= b
    // 用户化简原则（20260428180545）：等价逻辑下越简单越正确。
    // 这是纯布尔代数/比较运算反演，不改变 Java 求值语义。
    if let ExprKind::BinOp { op, left, right } = operand.kind() {
        if let Some(inv) = op.complemented_comparison() {
            return Some(builder::binop_public(inv, left.clone(), right.clone()));
        }
    }
    // 德摩根律 NOT(X && Y && ...) → NOT X || NOT Y || ...
    if let ExprKind::BinOp {
        op: BinOpKind::And, ..
    } = operand.kind()
    {
        let conjuncts = flatten_and(operand);
        if conjuncts.len() >= 2 {
            let negated: Vec<Expr> = conjuncts.iter().map(|c| builder::not(c.clone())).collect();
            return Some(rebuild_or(&negated));
        }
    }
    // 德摩根律 NOT(X || Y || ...) → NOT X && NOT Y && ...
    if let ExprKind::BinOp {
        op: BinOpKind::Or, ..
    } = operand.kind()
    {
        let disjuncts = flatten_or(operand);
        if disjuncts.len() >= 2 {
            let negated: Vec<Expr> = disjuncts.iter().map(|d| builder::not(d.clone())).collect();
            return Some(rebuild_and(&negated));
        }
    }
    None
}

// =============================================================================
// 共识律消元
// =============================================================================

fn common_factor_complement_reduce(disjuncts: &[Expr]) -> Vec<Expr> {
    let mut current = disjuncts.to_vec();
    loop {
        let conjs: Vec<Vec<Expr>> = current.iter().map(flatten_and).collect();
        let n = current.len();
        let mut merged: Option<(usize, usize, Expr)> = None;

        'outer: for i in 0..n {
            for j in (i + 1)..n {
                let Some(common) = common_factor_if_remainders_complement(&conjs[i], &conjs[j])
                else {
                    continue;
                };
                merged = Some((i, j, common));
                break 'outer;
            }
        }

        let Some((i, j, common)) = merged else {
            return current;
        };

        let mut next = Vec::with_capacity(current.len() - 1);
        for (idx, expr) in current.into_iter().enumerate() {
            if idx != i && idx != j {
                next.push(expr);
            }
        }
        next.push(common);
        current = next;
    }
}

fn common_factor_if_remainders_complement(left: &[Expr], right: &[Expr]) -> Option<Expr> {
    let mut right_used = vec![false; right.len()];
    let mut common: Vec<Expr> = Vec::new();
    let mut left_remainder: Vec<Expr> = Vec::new();

    for l in left {
        if let Some(idx) = right
            .iter()
            .enumerate()
            .find_map(|(idx, r)| (!right_used[idx] && r == l).then_some(idx))
        {
            right_used[idx] = true;
            common.push(l.clone());
        } else {
            left_remainder.push(l.clone());
        }
    }

    if common.is_empty() || left_remainder.is_empty() {
        return None;
    }

    let right_remainder: Vec<Expr> = right
        .iter()
        .enumerate()
        .filter_map(|(idx, r)| (!right_used[idx]).then_some(r.clone()))
        .collect();
    if right_remainder.is_empty() {
        return None;
    }

    let left_expr = rebuild_and(&left_remainder);
    let right_expr = rebuild_and(&right_remainder);
    if profile_pair_can_skip_proof_check(&left_expr, &right_expr)
        || !is_complement(&left_expr, &right_expr)
    {
        return None;
    }

    Some(rebuild_and(&common))
}

// =============================================================================
// 逆吸收律（resolution absorption）
// =============================================================================

/// 逆吸收律：A || (!A ∧ X) → A || X
///
/// 对每个 anchor disjunct d_i，从其它 disjunct d_j 的 conjuncts 中删除与 d_i
/// 互补的因子。删完若 conjs_j 为空，d_j 退化为 true（整个 OR 即为 true）。
///
/// 不动点循环直到无变化。
fn resolution_absorb(disjuncts: &[Expr]) -> Vec<Expr> {
    let mut current: Vec<Expr> = disjuncts.to_vec();
    loop {
        let n = current.len();
        let mut changed = false;
        let mut next: Vec<Expr> = Vec::with_capacity(n);
        for j in 0..n {
            let conjs_j = flatten_and(&current[j]);
            let filtered: Vec<Expr> = conjs_j
                .iter()
                .filter(|c| {
                    !current.iter().enumerate().any(|(i, anchor)| {
                        i != j
                            && !profile_pair_can_skip_proof_check(anchor, c)
                            && is_complement(anchor, c)
                    })
                })
                .cloned()
                .collect();
            if filtered.len() < conjs_j.len() {
                changed = true;
                if filtered.is_empty() {
                    next.push(builder::lit_bool(true));
                } else {
                    next.push(rebuild_and(&filtered));
                }
            } else {
                next.push(current[j].clone());
            }
        }
        current = next;
        if !changed {
            break;
        }
    }
    current
}

// =============================================================================
// 互补检测（复用给多元规则）
// =============================================================================

/// 判断 a 和 b 是否互补：
/// 1. 语法级：a == not(b) 或 b == not(a)
/// 2. Func 级：两个 Call 的 FuncKey 通过 complement 字段关联，且参数相同
fn ordered_comparisons_contradict(a: &Expr, b: &Expr) -> bool {
    let Some((subject_a, interval_a)) = ordered_comparison_interval(a) else {
        return false;
    };
    let Some((subject_b, interval_b)) = ordered_comparison_interval(b) else {
        return false;
    };
    if subject_a != subject_b {
        return false;
    }
    intervals_are_disjoint(interval_a, interval_b)
}

fn solve_ordered_comparison_conjunction(items: &[Expr]) -> Option<Vec<Expr>> {
    let mut solved: Vec<Expr> = Vec::with_capacity(items.len());
    let mut changed = false;
    let mut consumed = vec![false; items.len()];

    for i in 0..items.len() {
        if consumed[i] {
            continue;
        }
        let Some((subject, mut interval)) = ordered_comparison_interval(&items[i]) else {
            continue;
        };

        let mut group = vec![i];
        for j in (i + 1)..items.len() {
            if consumed[j] {
                continue;
            }
            let Some((other_subject, other_interval)) = ordered_comparison_interval(&items[j])
            else {
                continue;
            };
            if other_subject != subject {
                continue;
            }
            interval = intersect_intervals(interval, other_interval);
            group.push(j);
        }

        if group.len() == 1 {
            continue;
        }

        if interval_is_empty(interval) {
            return Some(vec![builder::lit_bool(false)]);
        }

        for idx in &group {
            consumed[*idx] = true;
        }
        let constraints = interval_to_constraints(&subject, interval);
        let original_group: Vec<Expr> = group.iter().map(|idx| items[*idx].clone()).collect();
        changed = changed || constraints != original_group;
        solved.extend(constraints);
    }

    if !changed {
        return None;
    }

    for (idx, item) in items.iter().enumerate() {
        if !consumed[idx] {
            solved.push(item.clone());
        }
    }
    Some(solved)
}

fn intersect_intervals(left: Interval, right: Interval) -> Interval {
    Interval {
        lower: max_lower(left.lower, right.lower),
        upper: min_upper(left.upper, right.upper),
    }
}

fn interval_is_empty(interval: Interval) -> bool {
    let (Some(lower), Some(upper)) = (interval.lower, interval.upper) else {
        return false;
    };
    lower.value > upper.value
        || (lower.value == upper.value && (!lower.inclusive || !upper.inclusive))
}

fn interval_to_constraints(subject: &Expr, interval: Interval) -> Vec<Expr> {
    if let (Some(lower), Some(upper)) = (interval.lower, interval.upper) {
        if lower.value == upper.value && lower.inclusive && upper.inclusive {
            return vec![builder::eq(subject.clone(), builder::lit_int(lower.value))];
        }
    }

    let mut constraints = Vec::new();
    if let Some(lower) = interval.lower {
        let op = if lower.inclusive {
            BinOpKind::Ge
        } else {
            BinOpKind::Gt
        };
        constraints.push(builder::binop_public(
            op,
            subject.clone(),
            builder::lit_int(lower.value),
        ));
    }
    if let Some(upper) = interval.upper {
        let op = if upper.inclusive {
            BinOpKind::Le
        } else {
            BinOpKind::Lt
        };
        constraints.push(builder::binop_public(
            op,
            subject.clone(),
            builder::lit_int(upper.value),
        ));
    }
    constraints
}

#[derive(Clone, Copy)]
enum EqualityAtomDomain { Literal, Allocated }

#[derive(PartialEq)]
enum EqualityAtom { Literal(Lit), Allocated(i64) }

fn equality_atom(expr: &Expr, domain: EqualityAtomDomain) -> Option<(BinOpKind, Expr, EqualityAtom)> {
    match domain {
        EqualityAtomDomain::Literal => literal_eq_ne_comparison(expr)
            .map(|(op, subject, value)| (op, subject, EqualityAtom::Literal(value))),
        EqualityAtomDomain::Allocated => heap_ref_eq_ne_comparison(expr)
            .filter(|(_, _, id)| *id > 0)
            .map(|(op, subject, id)| (op, subject, EqualityAtom::Allocated(id))),
    }
}

fn literal_eq_ne_comparison(expr: &Expr) -> Option<(BinOpKind, Expr, Lit)> {
    let ExprKind::BinOp { op, left, right } = expr.kind() else {
        return None;
    };
    if !matches!(op, BinOpKind::Eq | BinOpKind::Ne) {
        return None;
    }
    if let Some(lit) = right.as_lit() {
        return Some((*op, left.clone(), lit.clone()));
    }
    if let Some(lit) = left.as_lit() {
        return Some((*op, right.clone(), lit.clone()));
    }
    None
}

fn literal_equality_implies_literal_inequality(left: &Expr, right: &Expr) -> bool {
    let Some((left_op, left_subject, left_lit)) = literal_eq_ne_comparison(left) else {
        return false;
    };
    if left_op != BinOpKind::Eq {
        return false;
    }
    let Some((right_op, right_subject, right_lit)) = literal_eq_ne_comparison(right) else {
        return false;
    };
    right_op == BinOpKind::Ne && right_subject == left_subject && right_lit != left_lit
}

/// Extract a HeapRef equality/inequality comparison: `x == HeapRef(id)` or `x != HeapRef(id)`.
fn heap_ref_eq_ne_comparison(expr: &Expr) -> Option<(BinOpKind, Expr, i64)> {
    let ExprKind::BinOp { op, left, right } = expr.kind() else {
        return None;
    };
    if !matches!(op, BinOpKind::Eq | BinOpKind::Ne) {
        return None;
    }
    if let Some(id) = right.as_heap_ref() {
        return Some((*op, left.clone(), id));
    }
    if let Some(id) = left.as_heap_ref() {
        return Some((*op, right.clone(), id));
    }
    None
}

/// Lift an already-proven leaf implication over every alternative of an OR.
fn or_all_branches_imply(or_expr: &Expr, target: &Expr) -> bool {
    let disjuncts = flatten_or(or_expr);
    if disjuncts.len() <= 1 {
        return false;
    }
    disjuncts.iter().all(|d| {
        d == target || literal_equality_implies_literal_inequality(d, target)
    })
}

#[derive(Clone, Copy)]
struct Bound {
    value: i64,
    inclusive: bool,
}

#[derive(Clone, Copy)]
struct Interval {
    lower: Option<Bound>,
    upper: Option<Bound>,
}

fn ordered_comparison_interval(expr: &Expr) -> Option<(Expr, Interval)> {
    let (op, left, right) = expr.as_binop()?;
    let (op, subject, value) = if let Some(Lit::Int(n)) = right.as_lit() {
        (op, left, *n)
    } else if let Some(Lit::Int(n)) = left.as_lit() {
        (op.swapped_comparison()?, right, *n)
    } else {
        return None;
    };
    let bound = |inclusive| Some(Bound { value, inclusive });
    use BinOpKind::*;
    let (lower, upper) = match op {
        Lt => (None, bound(false)),
        Le => (None, bound(true)),
        Gt => (bound(false), None),
        Ge => (bound(true), None),
        Eq => (bound(true), bound(true)),
        _ => return None,
    };
    Some((subject.clone(), Interval { lower, upper }))
}

fn intervals_are_disjoint(a: Interval, b: Interval) -> bool {
    if let (Some(lower), Some(upper)) = (max_lower(a.lower, b.lower), min_upper(a.upper, b.upper)) {
        if lower.value > upper.value {
            return true;
        }
        if lower.value == upper.value && (!lower.inclusive || !upper.inclusive) {
            return true;
        }
    }
    false
}

fn max_lower(a: Option<Bound>, b: Option<Bound>) -> Option<Bound> {
    match (a, b) {
        (Some(x), Some(y)) => {
            if x.value > y.value {
                Some(x)
            } else if y.value > x.value {
                Some(y)
            } else {
                Some(Bound {
                    value: x.value,
                    inclusive: x.inclusive && y.inclusive,
                })
            }
        }
        (Some(x), None) | (None, Some(x)) => Some(x),
        (None, None) => None,
    }
}

fn min_upper(a: Option<Bound>, b: Option<Bound>) -> Option<Bound> {
    match (a, b) {
        (Some(x), Some(y)) => {
            if x.value < y.value {
                Some(x)
            } else if y.value < x.value {
                Some(y)
            } else {
                Some(Bound {
                    value: x.value,
                    inclusive: x.inclusive && y.inclusive,
                })
            }
        }
        (Some(x), None) | (None, Some(x)) => Some(x),
        (None, None) => None,
    }
}

fn comparison_or_absorb(disjuncts: &[Expr]) -> Vec<Expr> {
    let mut out = disjuncts.to_vec();
    let mut changed = true;
    while changed {
        changed = false;
        'outer: for i in 0..out.len() {
            for j in (i + 1)..out.len() {
                let Some(absorbed) = absorb_two_ordered_comparisons(&out[i], &out[j]) else {
                    continue;
                };
                out[i] = absorbed;
                out.remove(j);
                changed = true;
                break 'outer;
            }
        }
    }
    out
}

fn absorb_two_ordered_comparisons(a: &Expr, b: &Expr) -> Option<Expr> {
    let (
        ExprKind::BinOp {
            op: op_a,
            left: left_a,
            right: right_a,
        },
        ExprKind::BinOp {
            op: op_b,
            left: left_b,
            right: right_b,
        },
    ) = (a.kind(), b.kind())
    else {
        return None;
    };
    if left_a != left_b || op_a != op_b {
        return None;
    }
    let (Some(Lit::Int(ra)), Some(Lit::Int(rb))) = (right_a.as_lit(), right_b.as_lit()) else {
        return None;
    };

    let rhs = match op_a {
        BinOpKind::Le | BinOpKind::Lt => builder::lit_int((*ra).max(*rb)),
        BinOpKind::Ge | BinOpKind::Gt => builder::lit_int((*ra).min(*rb)),
        _ => return None,
    };
    Some(builder::binop_public(*op_a, left_a.clone(), rhs))
}

fn is_complement(a: &Expr, b: &Expr) -> bool {
    // 1. 语法级
    if let ExprKind::UnaryOp {
        op: UnaryOpKind::Not,
        operand,
    } = a.kind()
    {
        if operand == b {
            return true;
        }
    }
    if let ExprKind::UnaryOp {
        op: UnaryOpKind::Not,
        operand,
    } = b.kind()
    {
        if operand == a {
            return true;
        }
    }
    // 2. 比较级：`a < b` 与 `a >= b`、`x == null` 与 `x != null` 是互补。
    //
    // NOT may already have been rewritten as a closed comparison.
    if a.is_comparison_complement(b) { return true; }

    // 2b. De Morgan 互补：`A || B` 与 `!A && !B` 互补，反向同理。
    //
    // 这是纯布尔代数，不是 display/order 规则。它让分配律能识别
    // `(A || B) && ((!A && !B) || C)` 中第一项互补，从而化简为
    // `(A || B) && C`，避免 path guard 把前序失败条件反复膨胀。
    if is_demorgan_complement(a, b) {
        return true;
    }

    false
}

fn predicate_implies_nonnull_subject(expr: &Expr) -> Option<Expr> {
    if let Some((_key, subject)) = negated_call_subject_with_null_result(expr, true) {
        return Some(subject);
    }
    if let Some((_key, subject)) = call_subject_with_null_result(expr, false) {
        return Some(subject);
    }
    None
}

fn predicate_covers_null_subject(expr: &Expr) -> Option<Expr> {
    if let Some((_key, subject)) = call_subject_with_null_result(expr, true) {
        return Some(subject);
    }
    if let Some((_key, subject)) = negated_call_subject_with_null_result(expr, false) {
        return Some(subject);
    }
    None
}

fn null_comparison_subject(expr: &Expr, expected_op: BinOpKind) -> Option<Expr> {
    if let ExprKind::BinOp { op, left, right } = expr.kind() {
        if *op != expected_op {
            return None;
        }
        if right.is_null() {
            return Some(left.clone());
        }
        if left.is_null() {
            return Some(right.clone());
        }
    }
    None
}

fn negated_call_subject_with_null_result(
    expr: &Expr,
    expected: bool,
) -> Option<(crate::domains::primitive::registry::FuncKey, Expr)> {
    if let ExprKind::UnaryOp {
        op: UnaryOpKind::Not,
        operand,
    } = expr.kind()
    {
        return call_subject_with_null_result(operand, expected);
    }
    None
}

fn call_subject_with_null_result(
    expr: &Expr,
    expected: bool,
) -> Option<(crate::domains::primitive::registry::FuncKey, Expr)> {
    if let ExprKind::Call { key, args, .. } = expr.kind() {
        if args.len() != 1 {
            return None;
        }
        let sem = crate::domains::primitive::registry::func_semantics(*key);
        let null_behavior = sem.null_behavior?;
        if null_behavior.trigger_positions.contains(&0)
            && matches!(null_behavior.result, Lit::Bool(v) if v == expected)
        {
            return Some((*key, args[0].clone()));
        }
    }
    None
}

fn is_demorgan_complement(a: &Expr, b: &Expr) -> bool {
    let a_conjuncts = flatten_and(a);
    let b_disjuncts = flatten_or(b);
    if a_conjuncts.len() > 1
        && b_disjuncts.len() > 1
        && terms_are_pairwise_complements(&a_conjuncts, &b_disjuncts)
    {
        return true;
    }

    let a_disjuncts = flatten_or(a);
    let b_conjuncts = flatten_and(b);
    a_disjuncts.len() > 1
        && b_conjuncts.len() > 1
        && terms_are_pairwise_complements(&a_disjuncts, &b_conjuncts)
}

fn terms_are_pairwise_complements(left: &[Expr], right: &[Expr]) -> bool {
    if left.len() != right.len() {
        return false;
    }

    let mut used = vec![false; right.len()];
    for l in left {
        let Some(idx) = right
            .iter()
            .enumerate()
            .find_map(|(idx, r)| (!used[idx] && is_complement(l, r)).then_some(idx))
        else {
            return false;
        };
        used[idx] = true;
    }
    true
}
