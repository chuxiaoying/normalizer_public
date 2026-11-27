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
//! 先展开同族连接，再按有效域消去冗余，最后用共享结构键固定次序。

use super::logical_chain::{flatten_and, flatten_or, rebuild_and, rebuild_or};
use crate::{builder, domains::completion::different_throw_tags};
use crate::expr::{BinOpKind, Expr, ExprKind, Lit, RelationAggregateKind, UnaryOpKind};
use implication::*;

pub(crate) fn simplify(expr: &Expr, ctx: &dyn crate::sort_context::SortContext) -> Expr {
    if let Some((op @ (BinOpKind::And | BinOpKind::Or), _, _)) = expr.as_binop() {
        if expr.may_observe_sql_unknown() {
            // Only use the identities that are valid under SQL's three
            // values. IS NULL does not imply that inequality with text
            // is true: the conjunction can still evaluate to UNKNOWN.
            let (absorber, terms, rebuild): (bool, _, fn(&[Expr]) -> Expr) =
                if op == BinOpKind::And { (false, flatten_and(expr), rebuild_and) }
                else { (true, flatten_or(expr), rebuild_or) };
            if terms.iter().any(|term| term.as_bool() == Some(absorber)) {
                return builder::lit_bool(absorber);
            }
            let len = terms.len();
            let remaining: Vec<_> = terms.into_iter().filter(|term| term.as_bool().is_none()).collect();
            return if remaining.len() < len { rebuild(&remaining) } else { expr.clone() };
        }
    }
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
            // Binary orientation already used the shared structural key.
            // A second ordering by heap allocation ID would undo that
            // decision for named entry references.
            expr.clone()
        }
        ExprKind::UnaryOp {
            op: UnaryOpKind::Not,
            operand,
        } => negate_complete_boolean_case(operand, expr, ctx)
            .or_else(|| simplify_not(operand)).unwrap_or_else(|| expr.clone()),
        _ => expr.clone(),
    }
}

fn boolean_literal_equality(op: BinOpKind, left: &Expr, right: &Expr) -> Option<Expr> {
    let (op, value, literal) =
        super::constraint::comparison_literal_rhs((op, left, right), Expr::as_bool)?;
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
    if context.is_empty() {
        return None;
    }
    // The sole caller has matched a binary OR root.
    let disjuncts = flatten_or(or_expr);

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

        kept.push(disjunct);
    }

    changed.then(|| rebuild_or(&kept))
}

fn expr_is_true_under_and_context(expr: &Expr, context: &[Expr]) -> bool {
    if context.iter().any(|c| {
        c == expr
            || (!profile_pair_can_skip_proof_check(c, expr)
                && (relation_count_nonzero_implies_exists(c, expr)
                    || literal_equality_implies_literal_inequality(c, expr)
                    || expr.as_not().is_some_and(|negated| different_throw_tags(c, negated))
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
    if context.iter().any(|c| {
        !profile_pair_can_skip_proof_check(c, expr)
            && (is_complement(c, expr) || different_throw_tags(c, expr)
                || empty_input_implies_empty_selection(c, &builder::not(expr.clone())))
    }) {
        return true;
    }
    // 线性约束矛盾检测：x > 4 下 x < 4 恒 false
    if super::constraint::expr_contradicts_linear_context(expr, context) {
        return true;
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
            if common.is_empty() {
                continue;
            }

            let right_rest: Vec<Expr> = right_or
                .iter()
                .filter(|right| !common.iter().any(|common| common == *right))
                .cloned()
                .collect();
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

            return Some(replacement);
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
        // The context proof checks direct complements and exact Count/Exists
        // contradictions on this same pair before checking nested conditions.
        if expr_is_false_under_and_context(item, &out)
            || out.iter().any(|existing| {
                expr_is_false_under_and_context(existing, std::slice::from_ref(item))
            })
        {
            return Some(builder::lit_bool(false));
        }

        // A positive tag establishes the same receipt and excludes every
        // other class. Drop only those negations; never treat distinct
        // receipts as correlated or expand protected values into OR.
        if item.as_not().is_some_and(|negated| out.iter().any(|fact| different_throw_tags(fact, negated))) {
            continue;
        }
        out.retain(|fact| !fact.as_not().is_some_and(|negated| different_throw_tags(item, negated)));

        // A concrete literal and a positive allocation identity each form
        // their own closed equality class. Do not cross the domains.
        let mut skip_item = false;
        for domain in [EqualityAtomDomain::Literal, EqualityAtomDomain::Allocated] {
            let Some((op, subject, atom)) = equality_atom(item, domain) else { continue; };
            if op == BinOpKind::Eq {
                out.retain(|existing| {
                    let Some((op, other_subject, other)) = equality_atom(existing, domain) else { return true; };
                    if other_subject != subject { return true; }
                    op == BinOpKind::Eq || atom == other
                });
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

        if let Some(predicate_subject) = predicate_subject_at_null(item, false) {
            discard_null_guard(&mut out, BinOpKind::Ne, &predicate_subject);
        }

        out.push(item.clone());
    }

    // Narrow RCL cardinality absorption:
    //   Count(R) > 0  <=>  Exists(R)
    //
    // In AND contexts keep the stronger/arithmetic Count fact and remove the
    // duplicate Exists observer for the same relation. This is relation
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

    // Solve each independently acquired integer's conjunction on its
    // own domain. A separate total Boolean flag is not an interval term
    // and does not prevent exact intersection or point exclusion.
    let mut integer_groups = Vec::<(Expr, Vec<Expr>)>::new();
    let mut ordinary = Vec::new();
    for factor in &out {
        if let Some(subject) = super::integer_ranges::closed_integer_subject(factor) {
            if let Some((_, group)) = integer_groups.iter_mut().find(|(old, _)| *old == subject) {
                group.push(factor.clone());
            } else {
                integer_groups.push((subject, vec![factor.clone()]));
            }
        } else {
            ordinary.push(factor.clone());
        }
    }
    let mut shrunk = false;
    for (_, group) in integer_groups {
        let original = rebuild_and(&group);
        let mut replacement = None;
        if group.len() > 1 {
            replacement = super::integer_ranges::canonical_guard(&original, None, ctx)
                .filter(|value| crate::canonical_order::canonical_cost_compare(value, &original, ctx).is_lt());
        }
        if let Some(replacement) = replacement {
            ordinary.push(replacement);
            shrunk = true;
        } else {
            ordinary.extend(group);
        }
    }
    if shrunk { return Some(rebuild_and(&sort_commutative(ordinary, ctx))); }

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

    if out.len() < 2 {
        return Some(rebuild_and(&out));
    }
    // AND 和 OR 共用统一定形规范的结构排序键。
    let out = sort_commutative(out, ctx);
    // Sorting the leaves is not enough: differently associated trees can
    // have identical flat order. Compare the complete canonical left fold.
    let canonical = rebuild_and(&out);
    (canonical != *expr).then_some(canonical)
}

// =============================================================================
// 多元 OR 化简
// =============================================================================

fn boolean_parameter(expr: &Expr) -> bool {
    matches!(expr.as_sym_var(), Some(crate::SymVarOrigin::Parameter { name, primitive_type })
        if primitive_type == "boolean" && !name.starts_with('$') && !name.contains(['.', '[', ']']))
}

/// After separating the result from the path predicates used to discover
/// it, prefer a smaller closed Boolean value over its expanded proof form.
pub fn compact_total_input_boolean_value(expr: Expr, ctx: &dyn crate::SortContext) -> Expr {
    let Some((op @ (BinOpKind::And | BinOpKind::Or), ..)) = expr.as_binop() else { return expr; };
    let terms = if op == BinOpKind::And { flatten_and(&expr) } else { flatten_or(&expr) };
    let Some(terms) = terms.iter().map(|term| term.as_not().filter(|base| boolean_parameter(base)).cloned())
        .collect::<Option<Vec<_>>>()
    else { return expr; };
    let opposite = if op == BinOpKind::And { rebuild_or(&terms) } else { rebuild_and(&terms) };
    let candidate = builder::not(crate::normalize_with(opposite, ctx));
    if crate::canonical_order_compare(&candidate, &expr, ctx).is_lt() { candidate } else { expr }
}

/// Two complementary selector arms with complementary Boolean results
/// select equality (or inequality) of the selectors. A shared consequence
/// stays outside that equality: p ? (q OR c) : (!q OR c) == (p == q) OR c.
fn diagonal_boolean_pair(left: &Expr, right: &Expr) -> Option<Expr> {
    fn sign(expr: &Expr) -> (&Expr, bool) {
        expr.as_not().map_or((expr, true), |base| (base, false))
    }
    let (BinOpKind::And, a, b) = left.as_binop()? else { return None; };
    let (BinOpKind::And, c, d) = right.as_binop()? else { return None; };
    for (selector, arm) in [(a, b), (b, a)] {
        for (opposite, alternate) in [(c, d), (d, c)] {
            let (p, positive) = sign(selector);
            let (other_p, opposite_positive) = sign(opposite);
            if p != other_p || positive == opposite_positive || !boolean_parameter(p) { continue; }
            let left_terms = flatten_or(arm);
            let right_terms = flatten_or(alternate);
            let shared: Vec<_> = left_terms.iter().filter(|x| right_terms.contains(x)).cloned().collect();
            let left_only: Vec<_> = left_terms.iter().filter(|x| !shared.contains(x)).collect();
            let right_only: Vec<_> = right_terms.iter().filter(|x| !shared.contains(x)).collect();
            let ([l], [r]) = (left_only.as_slice(), right_only.as_slice()) else { continue; };
            let (q, q_positive) = sign(l);
            let (other_q, other_q_positive) = sign(r);
            if q != other_q || q_positive == other_q_positive || !boolean_parameter(q) { continue; }
            let guard = if positive == q_positive { builder::eq } else { builder::ne };
            let mut values = shared;
            values.push(guard(p.clone(), q.clone()));
            return Some(rebuild_or(&values));
        }
    }
    None
}

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
        // 互补检测：x || NOT(x) → true.
        if out.iter().any(|e| !profile_pair_can_skip_proof_check(e, item) && is_complement(e, item)) {
            return Some(builder::lit_bool(true));
        }
        // Primitive predicate null-behavior absorption:
        //   x == null || isBlank(x)   ->  isBlank(x), when isBlank(null)=true
        //   x == null || !isPresent(x)->  !isPresent(x), when isPresent(null)=false
        //
        // This is a math-level consequence of registered Primitive Function
        // null_behavior.  The predicate already covers the null case, so the
        // explicit Java implementation guard is redundant.  Do not encode
        // function names here; only consume FuncSemantics.
        if let Some(predicate_subject) = predicate_subject_at_null(item, true) {
            discard_null_guard(&mut out, BinOpKind::Eq, &predicate_subject);
        }
        out.push(item.clone());
    }

    if out.len() < 2 {
        return Some(rebuild_or(&out));
    }

    // All operands of this IR OR are acquired values. Protected reads
    // still reside in CASE rather than here; SQL UNKNOWN was excluded at
    // the public entry to simplify.
    let mut best: Option<Expr> = None;
    for i in 0..out.len() {
        for j in (i + 1)..out.len() {
            let factored = diagonal_boolean_pair(&out[i], &out[j]).or_else(|| {
                (super::boolean_domains::is_closed_boolean(&out[i])
                    && super::boolean_domains::is_closed_boolean(&out[j]))
                    .then(|| factor_common_and_conjuncts_under_or(&[out[i].clone(), out[j].clone()]))
                    .flatten()
            });
            if let Some(diagonal) = factored {
                let mut candidate: Vec<_> = out.iter().enumerate()
                    .filter_map(|(index, term)| (i != index && j != index).then(|| term.clone())).collect();
                candidate.push(diagonal);
                let candidate = rebuild_or(&candidate);
                if crate::canonical_order_compare(&candidate, best.as_ref().unwrap_or(expr), ctx).is_lt() {
                    best = Some(candidate);
                }
            }
        }
    }
    if best.is_some() { return best; }
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

fn negate_complete_boolean_case(operand: &Expr, original: &Expr, ctx: &dyn crate::SortContext) -> Option<Expr> {
    let (arms, fallback) = operand.as_total_case_when()?;
    if !arms.iter().all(|(_, value)| super::boolean_casewhen::is_boolean_expr(value))
        || !super::boolean_casewhen::is_boolean_expr(fallback)
    { return None; }
    // Only the subvalues are negated. Branch selection and the live
    // domain of any guarded receipt remain exactly the same.
    let invert = |value: &Expr| crate::normalize_with(builder::not(value.clone()), ctx);
    let candidate = builder::case_when(arms.iter().map(|(guard, value)|
        (guard.clone(), invert(value))).collect(), invert(fallback));
    crate::canonical_order_compare(&candidate, original, ctx).is_lt().then_some(candidate)
}

fn simplify_not(operand: &Expr) -> Option<Expr> {

    // not(not(x)) => x
    if let ExprKind::UnaryOp {
        op: UnaryOpKind::Not,
        operand: inner,
    } = operand.kind()
    {
        return Some(inner.clone());
    }
    if let Some(outcome) = crate::domains::completion::failure_domain_of_typed_return(operand) {
        return Some(outcome);
    }
    if let Some(inverse) = operand.comparison_complement_operator() {
        let (_, left, right) = operand.as_binop().expect("comparison contract");
        return Some(builder::binop_public(inverse, left.clone(), right.clone()));
    }
    // A matching binary root always flattens to at least two terms.
    let (op, _, _) = operand.as_binop()?;
    let (terms, rebuild): (_, fn(&[Expr]) -> Expr) = match op {
        BinOpKind::And => (flatten_and(operand), rebuild_or),
        BinOpKind::Or => (flatten_or(operand), rebuild_and),
        _ => return None,
    };
    let negated: Vec<_> = terms.into_iter().map(builder::not).collect();
    Some(rebuild(&negated))
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
                // Each disjunct only needs to explain the domain in which
                // its siblings are false. In p OR (p == q), the second
                // disjunct is therefore evaluated with p fixed to false.
                let replacement = current[j].as_binop()
                    .filter(|(op, left, right)| matches!(op, BinOpKind::Eq | BinOpKind::Ne)
                        && boolean_parameter(left) && boolean_parameter(right))
                    .and_then(|(op, left, right)| current.iter().enumerate().find_map(|(i, anchor)| {
                        if i == j { return None; }
                        let (subject, fixed) = anchor.as_not().map_or((anchor, false), |part| (part, true));
                        let remaining = if subject == left { right }
                            else if subject == right { left } else { return None; };
                        Some(if fixed == (op == BinOpKind::Eq) {
                            remaining.clone()
                        } else { builder::not(remaining.clone()) })
                    }));
                changed |= replacement.is_some();
                next.push(replacement.unwrap_or_else(|| current[j].clone()));
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

        // These shared-subject bounds can be intersected exactly even when
        // an earlier optional contextual proof declined a contradiction.
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

    // Terms outside a solved multi-bound group remain conjuncts, including
    // single bounds on different subjects and non-comparison predicates.
    solved.extend(items.iter().zip(consumed).filter_map(|(item, used)| (!used).then(|| item.clone())));
    Some(solved)
}

fn intersect_intervals(left: Interval, right: Interval) -> Interval {
    Interval {
        lower: tighter_bound(left.lower, right.lower, std::cmp::Ordering::Greater),
        upper: tighter_bound(left.upper, right.upper, std::cmp::Ordering::Less),
    }
}

fn interval_to_constraints(subject: &Expr, interval: Interval) -> Vec<Expr> {
    if let (Some(lower), Some(upper)) = (interval.lower, interval.upper) {
        // Over exact integers, open endpoints can also delimit a
        // singleton (9 < x + y < 11). Only use the shifted endpoints
        // to prove emptiness or a point; keep the original representatives
        // for a wider interval so the endpoint rules cannot oscillate.
        if super::integer_ac::total_integer(subject) {
            let first = crate::Integer::from(lower.value) + i32::from(!lower.inclusive);
            let last = crate::Integer::from(upper.value) - i32::from(!upper.inclusive);
            match first.cmp(&last) {
                std::cmp::Ordering::Greater => return vec![builder::lit_bool(false)],
                std::cmp::Ordering::Equal => return vec![builder::eq(subject.clone(), builder::lit_integer(first))],
                std::cmp::Ordering::Less => (),
            }
        }
        match lower.value.cmp(&upper.value) {
            std::cmp::Ordering::Greater => return vec![builder::lit_bool(false)],
            std::cmp::Ordering::Equal => return vec![if lower.inclusive && upper.inclusive {
                builder::eq(subject.clone(), builder::lit_int(lower.value))
            } else {
                builder::lit_bool(false)
            }],
            std::cmp::Ordering::Less => (),
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

fn eq_ne_comparison<T>(
    expr: &Expr, operand: impl Fn(&Expr) -> Option<T>,
) -> Option<(BinOpKind, Expr, T)> {
    let comparison @ (BinOpKind::Eq | BinOpKind::Ne, ..) = expr.as_binop()? else {
        return None;
    };
    let (op, subject, value) = super::constraint::comparison_literal_rhs(comparison, operand)?;
    Some((op, subject.clone(), value))
}

fn literal_eq_ne_comparison(expr: &Expr) -> Option<(BinOpKind, Expr, Lit)> {
    eq_ne_comparison(expr, |operand| operand.as_lit().cloned())
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
    eq_ne_comparison(expr, Expr::as_heap_ref)
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
    let (op, subject, value) = super::constraint::comparison_literal_rhs(
        expr.as_binop()?,
        super::constraint::compact_integer,
    )?;
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

fn tighter_bound(
    a: Option<Bound>, b: Option<Bound>, preferred: std::cmp::Ordering,
) -> Option<Bound> {
    match (a, b) {
        (Some(a), Some(b)) => Some(match a.value.cmp(&b.value) {
            std::cmp::Ordering::Equal => Bound {
                value: a.value,
                inclusive: a.inclusive && b.inclusive,
            },
            order if order == preferred => a,
            _ => b,
        }),
        (Some(bound), None) | (None, Some(bound)) => Some(bound),
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
                let contained = if empty_relation_is_within_count(&out[i], &out[j]) {
                    Some(out[j].clone())
                } else if empty_relation_is_within_count(&out[j], &out[i]) {
                    Some(out[i].clone())
                } else { None };
                let Some(absorbed) = contained.or_else(|| absorb_two_ordered_comparisons(&out[i], &out[j])) else {
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
        _ => return None,
    };
    Some(builder::binop_public(*op_a, left_a.clone(), rhs))
}

fn is_complement(a: &Expr, b: &Expr) -> bool {
    if crate::domains::completion::single_failure_complement(a, b) { return true; }
    // SQL UNKNOWN survives negation, so a syntactic NOT does not prove
    // excluded middle or contradiction for a nullable row predicate.
    if let ExprKind::UnaryOp {
        op: UnaryOpKind::Not,
        operand,
    } = a.kind()
    {
        if operand == b {
            return !operand.may_observe_sql_unknown();
        }
    }
    if let ExprKind::UnaryOp {
        op: UnaryOpKind::Not,
        operand,
    } = b.kind()
    {
        if operand == a {
            return !operand.may_observe_sql_unknown();
        }
    }
    // 2. 比较级：`a < b` 与 `a >= b`、`x == null` 与 `x != null` 是互补。
    //
    // NOT may already have been rewritten as a closed comparison.
    if a.is_comparison_complement(b) || super::integer_ranges::complementary_guards(a, b) { return true; }

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

/// Match the registered Boolean result when the unary predicate's argument
/// is null. A single explicit NOT flips only the required result.
fn predicate_subject_at_null(expr: &Expr, expected: bool) -> Option<Expr> {
    let (call, expected) = match expr.as_unary_op() {
        Some((UnaryOpKind::Not, operand)) => (operand, !expected),
        _ => (expr, expected),
    };
    let (key, args) = call.as_call()?;
    let [subject] = args else { return None; };
    let behavior = crate::domains::primitive::registry::func_semantics(key).null_behavior?;
    (behavior.trigger_positions.contains(&0)
        && matches!(behavior.result, Lit::Bool(value) if value == expected))
        .then(|| subject.clone())
}

fn null_comparison_subject(expr: &Expr, expected_op: BinOpKind) -> Option<Expr> {
    let comparison = expr.as_binop()?;
    if comparison.0 != expected_op {
        return None;
    }
    super::constraint::comparison_literal_rhs(comparison, |value| value.is_null().then_some(()))
        .map(|(_, subject, ())| subject.clone())
}

fn discard_null_guard(terms: &mut Vec<Expr>, op: BinOpKind, subject: &Expr) {
    terms.retain(|term| {
        null_comparison_subject(term, op).as_ref().is_none_or(|guarded| guarded != subject)
    });
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
