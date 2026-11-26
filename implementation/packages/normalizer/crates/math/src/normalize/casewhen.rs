//! CaseWhen 化简规则。
//!
//! - 默认分支提前（第一个 condition 为 true 的分支即结果）
//! - 单分支退化：CaseWhen [(true, v)] → v
//! - 后续死分支消除：condition=false 的分支剔除
//! - 相邻互补 else 归一：CaseWhen [(c, v1), (NOT(c), v2)] / [(NOT(c), v1), (c, v2)] → [(first, v1), (true, v2)]
//! - 路径守卫冗余消除：branch[k].cond 里 AND 项匹配 NOT(branch[j].cond) (j<k) 剥除

use super::logical_chain::{flatten_and, flatten_or, rebuild_and, rebuild_or};
use crate::expr::{Expr, ExprKind, Lit, UnaryOpKind};
use std::collections::HashSet;

pub(crate) fn simplify(expr: &Expr, ctx: &dyn crate::sort_context::SortContext) -> Expr {
    let Some(branches) = expr.as_case_when() else {
        return expr.clone();
    };
    if let Some(value) = select_first_default(expr) {
        return value;
    }
    let start = (branches.len() > 1 && crate::profile::enabled()).then(std::time::Instant::now);
    let cost_gate = || branches.iter().fold(
        (0usize, 0usize), |(size, depth), (condition, value)| {
            (size.saturating_add(condition.size()).saturating_add(value.size()),
             depth.max(value.depth()))
        },
    );
    let output = simplify_impl(expr, branches, ctx);
    if let Some(start) = start {
        let (flatten_size, flatten_depth) = cost_gate();
        let skip_reason = if flatten_size > 32_768 || flatten_depth > 16 {
            Some("flatten_cost_gate")
        } else if output == *expr {
            Some("no_rewrite")
        } else {
            None
        };
        crate::profile::rule_event(
            "math.casewhen.group",
            expr.size(),
            output.size(),
            start.elapsed().as_micros(),
            skip_reason,
        );
    }
    output
}

/// The protected arm already evaluates the acquired reference's non-null
/// comparison first. Move the adjacent null arm ahead to express that
/// partition once; the protected guard and value still run only when nonnull.
/// Do not move a nullable field observation or a preceding guard.
fn factor_leading_null_test(branches: &[(Expr, Expr)]) -> Option<Vec<(Expr, Expr)>> {
    use crate::BinOpKind::{Eq, Ne};
    for (index, pair) in branches.windows(2).enumerate() {
        let [(protected, selected), (null, on_null)] = pair else { continue; };
        let Some((Eq, subject, nil)) = null.as_binop() else { continue; };
        if subject.as_heap_ref().is_none() || !nil.is_null() { continue; }
        let parts = flatten_and(protected);
        let Some((first, rest)) = parts.split_first() else { continue; };
        if rest.is_empty()
            || !matches!(first.as_binop(), Some((Ne, other, rhs)) if other == subject && rhs == nil)
        {
            continue;
        }
        let mut result = branches.to_vec();
        result[index] = (null.clone(), on_null.clone());
        result[index + 1] = (rebuild_and(rest), selected.clone());
        return Some(result);
    }
    None
}

fn simplify_impl(expr: &Expr, branches: &[(Expr, Expr)], ctx: &dyn crate::sort_context::SortContext) -> Expr {
    if let Some(value) = redundant_integer_point_arm(branches) {
        return value;
    }
    // An exhaustive binary partition may be complemented only together
    // with swapping its values. Use the shared predicate ordering so source
    // branch order cannot choose two different normal forms.
    if let Some(oriented) = crate::case_orientation::canonicalize_binary_case(expr, ctx) {
        return oriented;
    }

    // -1. 嵌套 CaseWhen 展平（最优先）：
    //    若 branch value 是 CaseWhen，按笛卡尔积展开为多个 branch：
    //    (outer_cond, CaseWhen[(ic1, v1), ..., (true, vn)])
    //    → (outer_cond && ic1, v1), ..., (outer_cond && true, vn) = (outer_cond, vn)
    //
    //    语义依据：CaseWhen 按序匹配；外层 cond 为真时进入内层再按序匹配，
    //    等价于把外层 cond 与每个内层 cond AND 起来作为独立 branch。
    //
    //    展开后产生的重复 AND 项（如 `a && a`）由 boolean 多元 dedupe 吃掉；
    //    冗余 path-guard 由 strip_redundant_path_guards 剥除；
    //    不动点迭代保证下一轮继续化简。
    // Flattening a value CASE duplicates its outer guard for every inner
    // arm. Keep the nested form once the complete branch tree is large;
    // this preserves first-match semantics and avoids exponential rebuilds
    // during fold substitution. Small trees retain the canonical flat form.
    // The cost gate must cover the proof-heavy CaseWhen rules below as
    // well as nested-value flattening.  A large branch payload can make
    // first-match domain construction and coverage proofs repeatedly
    // rebuild tens of thousands of nodes even when no rewrite is found.
    // Keeping the immutable CaseWhen is semantics-preserving and lets
    // the caller's resource budget remain available for the surrounding
    // expression.  Small cases retain the full canonicalization rules.
    let (flattened_branches, flatten_changed) = flatten_nested_value_branches(branches);
    let branches_ref: &[(Expr, Expr)] = if flatten_changed {
        &flattened_branches
    } else {
        branches
    };

    if let Some(guarded) = factor_leading_null_test(branches_ref) {
        return Expr::new(ExprKind::CaseWhen { branches: guarded });
    }

    if let Some((ranged, fallback)) = super::integer_ranges::case_ranges(branches_ref) {
        // Normalize guards before comparison to avoid commuting AND terms
        // forever between this rule and the normal boolean ordering.
        let arms: Vec<_> = ranged
            .into_iter()
            .map(|(c, v)| (crate::normalize_with(c, ctx), v))
            .collect();
        // The range calculation proves each effective domain, but an
        // ordered CASE only needs to state what prior failures do not
        // already imply. Run the same projection inside this rule so a
        // later range pass cannot restore and publish the redundant bound.
        let (guards, _) = strip_redundant_path_guards(&arms);
        return crate::builder::case_when(guards, fallback);
    }

    // 0. 路径守卫冗余消除（新增规则）：
    //    switch-case 语义本身保证：branch[k] 被匹配时，branch[0..k] 的 cond
    //    全部为 false。因此 branch[k].cond 中任何 AND 项若形如
    //    NOT(branch[j].cond) (j<k)，都是冗余守卫，可剥除。
    //
    // 例：expected `WHEN StringUtil.isBlank(code) THEN "code required"`
    //    actual   `WHEN StringUtil.isBlank(code) && NOT (StringUtil.isBlank(name)) THEN "code required"`
    //    剥除 NOT (StringUtil.isBlank(name))，因为 name isBlank 的分支已经在前面被匹配过
    //
    // 实现：对每个 branch，把 cond 展平成 AND conjuncts，过滤掉匹配前面任意
    // branch cond 的 NOT 项，再从剩余 conjuncts 重建 And 链（空→true）。
    let (guard_stripped, guard_changed) = strip_redundant_path_guards(branches_ref);

    // 0a. OR 分支冗余消除：
    //    CaseWhen 按序匹配，branch[k] 被检查时所有前序 cond 已失败。
    //    因此 branch[k].cond 中 OR 项若等于某个前序 cond，该 disjunct 在当前语境下恒 false，
    //    可以剥除。
    //
    //    例：CASE WHEN A THEN v1 WHEN A || B THEN v2 WHEN true THEN v3
    //       → CASE WHEN A THEN v1 WHEN B THEN v2 WHEN true THEN v3
    let (or_stripped, or_changed) = strip_prev_disjuncts_from_or_guards(&guard_stripped);

    // 0b. first-match prior-failure 域下的精确互补 guard 剥离：
    //    CaseWhen 按序匹配，branch[k] 被检查时所有前序 cond 已失败。
    //    因此前序 `WHEN NOT(A)` 失败后，later branch 中 `A && B` 的
    //    `A` conjunct 是冗余 guard，可剥为 `B`。
    //
    //    注意：这里只做 syntactic exact-complement conjunct stripping，
    //    不做全域 SAT/implication 化简；否则会把正常返回 guard 过宽吞掉。
    //    更强的代数律必须单独证明后沉淀到 math，而不是复刻 formatter 兜底。
    let (domain_simplified, domain_simplified_changed) =
        simplify_branch_conditions_under_prior_failures(&or_stripped);

    // 0c. 死分支消除（相同 cond 的后续 branch 不可达）：
    //    若 branch[k].cond 与某 branch[j].cond (j<k) 完全相等，
    //    则 branch[k] 永远不会被匹配（前面的必先命中），剔除。
    //
    //    这是 strip_redundant_path_guards 的自然推广：path-guard 剥的是
    //    NOT(prev_cond) AND 项；这里剥的是 cond 整体等于某 prev_cond。
    //
    //    例：CaseWhen [(c, v1), (c, v2), (true, v3)] → [(c, v1), (true, v3)]
    //    场景：Map.get 的两分支 return 路径在 merge 时产生 cond 相同但 value
    //    不同的 branch 序列（CaseWhen flatten 后更常见）。
    let (same_value_merged, same_value_merged_changed) =
        merge_adjacent_same_value_branches(&domain_simplified);

    let (same_value_grouped, same_value_grouped_changed) =
        group_same_value_effective_domains(&same_value_merged);

    // 0d. 覆盖死分支消除：
    //    CaseWhen 按序匹配，branch[k] 只有在所有前序 cond 都失败时才会被检查。
    //    若 `cond[k] && !cond[0] && ... && !cond[k-1]` 经 normalize 可证明为 false，
    //    则 branch[k] 已被前序分支全集覆盖，永远不可达。
    //
    //    典型场景：外层 guard 已证明 `value != null`，而 return value 是同一个
    //    `value` 的 CaseWhen。nested flatten 会生成默认 null 分支；该分支被前面
    //    非 null value 分支覆盖，应删除，而不是渲染成 `"found:" + null`。
    let (covered_stripped, covered_changed) = dead_branch_by_covered_prev_conds(&same_value_grouped);

    // Drop false guards and stop after the first explicit default.
    let mut filtered: Vec<(Expr, Expr)> = Vec::new();
    for (condition, value) in &covered_stripped {
        if condition.as_bool() == Some(false) {
            continue;
        }
        let is_default = condition.as_bool() == Some(true);
        filtered.push((condition.clone(), value.clone()));
        if is_default {
            break;
        }
    }

    // 3. 如果唯一分支是 (true, v)，退化为 v
    if filtered.len() == 1 {
        if let ExprKind::Lit(Lit::Bool(true)) = filtered[0].0.kind() {
            return filtered[0].1.clone();
        }
    }


    // 3b. Two-value exhaustive CaseWhen canonicalization:
    //
    //   CASE
    //     WHEN A THEN X
    //     WHEN B THEN Y
    //     WHEN C THEN X
    //     WHEN D THEN Y
    //   END
    //
    // If the first-match effective domains cover the whole space, and there
    // are exactly two payloads, display the more common / explicit else
    // payload as the default and keep only the other payload's domains:
    //
    //   CASE
    //     WHEN B THEN Y
    //     WHEN D THEN Y
    //     WHEN true THEN X
    //   END
    //
    // This is a semantic CaseWhen canonicalization, not a formatter shape
    // patch: it operates on first-match effective domains and only fires
    // after math proves exhaustive coverage.
    if let Some(collapsed) = collapse_two_value_exhaustive_to_default(&filtered) {
        return collapsed;
    }

    // 3b. 所有分支值结构相同 → 退化为该值（CaseWhen-MethodCall PR 衍生规则）
    //
    // 场景：MethodCall on CaseWhen receiver 分发后，每个 branch 调用同方法可能
    // 产生相同结果，此时 CaseWhen 包裹无意义，归一化为该值。

    // 4. 如果 flatten / dedup / canonicalize / 过滤 / 截断 / 剥守卫 导致变化，返回新 CaseWhen
    if flatten_changed
        || guard_changed
        || or_changed
        || domain_simplified_changed
        || same_value_merged_changed
        || same_value_grouped_changed
        || covered_changed
        // The earlier stages report any changes; this comparison certifies the
        // last filter/truncate step against its actual input.
        || filtered.len() != covered_stripped.len()
    {
        if filtered.is_empty() {
            // 不应该发生（builder 保证至少有 true 分支），防御
            return expr.clone();
        }
        return Expr::new(ExprKind::CaseWhen { branches: filtered });
    }
    expr.clone()
}

/// 嵌套 CaseWhen 展平：若 branch value 是 CaseWhen，展开为笛卡尔积。
///
/// `(outer_cond, CaseWhen[(ic1, v1), (ic2, v2), ..., (true, vn)])`
/// → `(outer_cond && ic1, v1), (outer_cond && ic2, v2), ..., (outer_cond, vn)`
///
/// 最后一个 inner branch 的 cond 一定是 `true`（builder::case_when 保证），
/// 所以 `outer_cond && true` 化简为 `outer_cond`，此处直接复用 outer_cond。
// At a certified exact integer point, a branch that matches the other
// total value is redundant. Do not substitute into partial operands,
// calls, relation bodies, floating point operations or word operators.
fn redundant_integer_point_arm(branches: &[(Expr, Expr)]) -> Option<Expr> {
    use crate::{builder as b, BinOpKind as B, Lit};
    let [(condition, yes), (always, no)] = branches else { return None; };
    if always.as_bool() != Some(true) { return None; }
    let (op, left, right) = condition.as_binop()?;
    let (point_value, generic_value) = match op {
        B::Eq => (yes, no),
        B::Ne => (no, yes),
        _ => return None,
    };
    let is_integer = |value: &Expr| value.as_lit().and_then(Lit::as_integer).is_some();
    let (subject, point) = if is_integer(right) { (left, right) } else { (right, left) };
    if subject.as_sym_var().is_none() || !is_integer(point)
        || !super::integer_ac::total_integer(subject)
        || !super::integer_ac::total_integer(generic_value)
        || !super::integer_ac::total_integer(point_value) {
        return None;
    }
    fn at(value: &Expr, subject: &Expr, point: &Expr) -> Expr {
        if value == subject { return point.clone(); }
        if let Some((op, left, right)) = value.as_binop() {
            if matches!(op, B::Add | B::Sub | B::Mul) {
                return b::binop_public(op, at(left, subject, point), at(right, subject, point));
            }
        }
        value.clone()
    }
    (crate::normalize(at(generic_value, subject, point)) == crate::normalize(at(point_value, subject, point)))
        .then(|| generic_value.clone())
}

fn collapse_two_value_exhaustive_to_default(branches: &[(Expr, Expr)]) -> Option<Expr> {
    let (last_cond, default_value) = branches.last()?;
    if last_cond.as_bool() != Some(true) {
        return None;
    }
    let mut values = branches.iter().map(|(_, value)| value);
    let distinct = values.find(|value| *value != default_value)?;
    if values.any(|value| value != default_value && value != distinct) {
        return None;
    }
    // The explicit TRUE arm certifies coverage. Effective domains are still
    // needed to preserve first-match priority when earlier guards overlap.
    let effective_domains = first_match_effective_domains(branches);
    let default_value = default_value.clone();

    let mut out: Vec<(Expr, Expr)> = Vec::new();
    for ((_, value), domain) in branches.iter().zip(effective_domains.into_iter()) {
        if value != &default_value {
            let displayed_domain = crate::normalize(domain);
            for disjunct in flatten_or(&displayed_domain) {
                out.push((disjunct, value.clone()));
            }
        }
    }
    out.push((crate::builder::lit_bool(true), default_value));
    if out == branches {
        return None;
    }
    Some(Expr::new(ExprKind::CaseWhen { branches: out }))
}

fn first_match_effective_domains(branches: &[(Expr, Expr)]) -> Vec<Expr> {
    let mut remaining = crate::builder::lit_bool(true);
    let mut out = Vec::with_capacity(branches.len());
    for (cond, _) in branches {
        let (effective, later) = crate::builder::first_match_domains(remaining, cond);
        out.push(effective);
        remaining = later;
    }
    out
}

fn flatten_nested_value_branches(branches: &[(Expr, Expr)]) -> (Vec<(Expr, Expr)>, bool) {
    let mut out: Vec<(Expr, Expr)> = Vec::with_capacity(branches.len());
    let mut changed = false;

    for (outer_cond, value) in branches.iter() {
        if let ExprKind::CaseWhen {
            branches: inner_branches,
        } = value.kind()
        {
            changed = true;
            for (inner_cond, inner_val) in inner_branches {
                let merged_cond = if inner_cond.as_bool() == Some(true) {
                    outer_cond.clone()
                } else {
                    crate::builder::and(outer_cond.clone(), inner_cond.clone())
                };
                out.push((merged_cond, inner_val.clone()));
            }
        } else {
            out.push((outer_cond.clone(), value.clone()));
        }
    }

    (out, changed)
}

/// OR 分支冗余消除：对每个 branch[k]，若 cond 是 OR 链，则剥除等于前序 cond 的 disjunct。
///
/// CaseWhen 按序匹配：进入 branch[k] 时，所有前序 branch cond 已经匹配失败。
/// 因此 `WHEN A THEN v1; WHEN A || B THEN v2` 的第二分支中 A 已知为 false，
/// 可化简为 `WHEN B THEN v2`。
fn map_prior_guards(
    branches: &[(Expr, Expr)],
    mut rewrite: impl FnMut(&Expr, &[&Expr]) -> Option<Expr>,
) -> (Vec<(Expr, Expr)>, bool) {
    let mut previous = Vec::with_capacity(branches.len());
    let mut out = Vec::with_capacity(branches.len());
    let mut changed = false;
    for (condition, value) in branches {
        let rewritten = rewrite(condition, &previous);
        changed |= rewritten.is_some();
        out.push((rewritten.unwrap_or_else(|| condition.clone()), value.clone()));
        previous.push(condition);
    }
    (out, changed)
}

fn strip_prev_disjuncts_from_or_guards(branches: &[(Expr, Expr)]) -> (Vec<(Expr, Expr)>, bool) {
    map_prior_guards(branches, |cond, prev_conds| {
        let previous_or = prev_conds
            .iter()
            .map(|prev_cond| (**prev_cond).clone())
            .reduce(crate::builder::or)
            .unwrap_or_else(|| crate::builder::lit_bool(false));
        let disjuncts = flatten_or(cond);
        let original_len = disjuncts.len();
        let filtered: Vec<Expr> = disjuncts
            .into_iter()
            .filter(|d| !prev_conds.iter().any(|prev| *prev == d))
            .filter(|d| !crate::builder::path_implies(d, &previous_or))
            .collect();
        (filtered.len() != original_len).then(|| rebuild_or(&filtered))
    })
}

fn merge_adjacent_same_value_branches(branches: &[(Expr, Expr)]) -> (Vec<(Expr, Expr)>, bool) {
    if branches.len() < 2 {
        return (branches.to_vec(), false);
    }

    let mut out: Vec<(Expr, Expr)> = Vec::with_capacity(branches.len());
    let mut changed = false;

    // Adjacent equal-payload arms preserve their first-match and short-circuit
    // order under an ordered OR: C1 ? V; C2 ? V; ... == (C1 || C2) ? V; ... .
    // Do not cross an intervening different payload; non-adjacent grouping is
    // handled separately using effective domains.
    for (cond, value) in branches.iter() {
        let normalized_value = crate::normalize(value.clone());
        if let Some((prev_cond, prev_value)) = out.last_mut() {
            if crate::normalize(prev_value.clone()) == normalized_value {
                let merged_cond =
                    crate::normalize(crate::builder::or(prev_cond.clone(), cond.clone()));
                *prev_cond = merged_cond;
                *prev_value = normalized_value;
                changed = true;
                continue;
            }
        }
        out.push((cond.clone(), normalized_value));
    }

    (out, changed)
}

fn group_same_value_effective_domains(branches: &[(Expr, Expr)]) -> (Vec<(Expr, Expr)>, bool) {
    if branches.len() < 3 {
        return (branches.to_vec(), false);
    }

    // An explicit default makes the union of all effective domains total.
    // Preserve that payload as the final default after grouping: all other
    // groups retain their exact first-match domains, so their complement is
    // precisely the original default-value domain (including earlier arms
    // with the same payload). Binary total partitions keep their established
    // orientation rule below; this extension introduces no value preference.
    let explicit_default = branches
        .last()
        .filter(|(cond, _)| cond.as_bool() == Some(true))
        .map(|(_, value)| crate::normalize(value.clone()));

    // Grouping can only remove an arm when at least one payload repeats.
    // In particular a total CASE already emitted by this rule has unique
    // payloads, so do not rebuild its effective domains on later iterations.
    if explicit_default.is_some() {
        let mut seen_values = HashSet::new();
        if branches
            .iter()
            .all(|(_, value)| seen_values.insert(crate::normalize(value.clone())))
        {
            return (branches.to_vec(), false);
        }
    }

    let effective_domains = first_match_effective_domains(branches);

    // If a partial CaseWhen is an exhaustive two-value partition, leave the
    // orientation decision to collapse_two_value_exhaustive_to_default.  That
    // rule produces the canonical "positive domains + true default" shape;
    // grouping both payloads here would steal that canonicalization and leave
    // a default-less two-arm CaseWhen.
    let raw_conds: Vec<Expr> = branches.iter().map(|(cond, _)| cond.clone()).collect();
    let raw_covered = crate::builder::conditions_cover_all(&raw_conds);
    let effective_covered = crate::builder::conditions_cover_all(&effective_domains);
    if raw_covered || effective_covered {
        let mut values: Vec<Expr> = Vec::new();
        for (_, value) in branches {
            let normalized_value = crate::normalize(value.clone());
            if !values.iter().any(|v| v == &normalized_value) {
                values.push(normalized_value);
            }
        }
        if values.len() == 2 {
            return (branches.to_vec(), false);
        }
    }

    let mut groups: Vec<(Expr, Vec<Expr>)> = Vec::new();

    for ((_, value), domain) in branches.iter().zip(effective_domains.into_iter()) {
        let domain = crate::normalize(domain);
        if domain.as_bool() == Some(false) {
            continue;
        }
        let normalized_value = crate::normalize(value.clone());
        if let Some((_, domains)) = groups
            .iter_mut()
            .find(|(group_value, _)| crate::normalize(group_value.clone()) == normalized_value)
        {
            domains.push(domain);
        } else {
            groups.push((normalized_value, vec![domain]));
        }
    }


    // Each input contributes to at most one nonempty group. Without
    // a repeated payload, grouping cannot eliminate an arm.
    if groups.iter().all(|(_, domains)| domains.len() == 1) {
        return (branches.to_vec(), false);
    }

    let mut out: Vec<(Expr, Expr)> = groups
        .into_iter()
        .map(|(value, domains)| {
            let cond = domains
                .into_iter()
                .reduce(crate::builder::or)
                .map(crate::normalize)
                .unwrap_or_else(|| crate::builder::lit_bool(false));
            (cond, value)
        })
        .collect();

    if let Some(default) = explicit_default {
        // Do not OR raw branch guards: moving a later same-value arm before
        // intervening different values is valid only for effective domains.
        // Moving the default group last is safe for the same disjointness
        // reason. Preserve first occurrence among the non-default groups;
        // this is this rule's rewrite direction, not a claim that every
        // equivalent partition already has the same global normal form.
        // Each payload occurs once afterwards, and the arm count strictly
        // decreases, so this rule cannot regroup its own output.
        if let Some(index) = out.iter().position(|(_, value)| value == &default) {
            out.remove(index);
        }
        out.push((crate::builder::lit_bool(true), default));
    }

    // A merged reachable group removes one arm. If the old default was
    // unreachable, losing that original arm pays for reattaching it.
    (out, true)
}

fn simplify_branch_conditions_under_prior_failures(branches: &[(Expr, Expr)]) -> (Vec<(Expr, Expr)>, bool) {
    map_prior_guards(branches, |cond, prev_conds| {
        if cond.as_bool() == Some(true) {
            return None;
        }
        let conjuncts = flatten_and(cond);
        let original_len = conjuncts.len();
        let filtered: Vec<Expr> = conjuncts
            .into_iter()
            .filter(|conj| !prev_conds.iter().any(|prev| is_not_of(prev, conj)))
            .collect();
        (filtered.len() != original_len).then(|| rebuild_and(&filtered))
    })
}

fn dead_branch_by_covered_prev_conds(branches: &[(Expr, Expr)]) -> (Vec<(Expr, Expr)>, bool) {
    let mut out: Vec<(Expr, Expr)> = Vec::with_capacity(branches.len());
    let mut seen = HashSet::with_capacity(branches.len());
    for (cond, value) in branches {
        // Remember even earlier guards removed by the proof. An exact
        // duplicate never enters the semantic prover a second time.
        if seen.insert(cond) && !is_covered_by_previous(cond, &out) {
            out.push((cond.clone(), value.clone()));
        }
    }
    let changed = out.len() != branches.len();
    (out, changed)
}

fn is_covered_by_previous(cond: &Expr, previous: &[(Expr, Expr)]) -> bool {
    let Some(((first, _), rest)) = previous.split_first() else {
        return false;
    };

    if cond.as_bool() == Some(true) {
        let previous_conds: Vec<Expr> = previous.iter().map(|(guard, _)| guard.clone()).collect();
        return crate::builder::conditions_cover_all(&previous_conds);
    }

    let previous_or = rest.iter().fold(first.clone(), |prefix, (guard, _)| {
        crate::builder::or(prefix, guard.clone())
    });
    crate::builder::path_implies(cond, &previous_or)
}

/// 路径守卫冗余消除：对每个 branch[k]，剥除 cond 中形如 NOT(branch[j].cond) (j<k) 的 AND 项。
///
/// switch-case 按序匹配语义：branch[k] 被选中时，branch[0..k].cond 全部为 false。
/// 因此 branch[k].cond 显式包含 NOT(prev_cond) 作为 AND 项是冗余的。
fn strip_redundant_path_guards(branches: &[(Expr, Expr)]) -> (Vec<(Expr, Expr)>, bool) {
    map_prior_guards(branches, |cond, prev_conds| {
        let conjuncts = flatten_and(cond);
        let original_len = conjuncts.len();
        let filtered: Vec<Expr> = conjuncts
            .iter()
            .filter(|c| {
                !is_not_of_any(c, prev_conds)
                    && !is_not_conj_implied_by_prev_failure(c, &conjuncts, prev_conds)
            })
            .cloned()
            .collect();
        (filtered.len() != original_len).then(|| rebuild_and(&filtered))
    })
}

/// 检查 cond 是否是 NOT(expected) 对任一 prev_cond 成立。
fn is_not_of_any(cond: &Expr, prev_conds: &[&Expr]) -> bool {
    for prev in prev_conds {
        if is_not_of(cond, prev)
            || flatten_or(prev).iter().any(|disjunct| is_not_of(cond, disjunct))
        {
            return true;
        }
    }
    false
}

/// 检查当前分支中的 `NOT(x)` 是否可由“前序分支匹配失败 + 当前其它 conjuncts”推出。
///
/// CaseWhen 按序匹配：进入 branch[k] 时，所有 prev_cond 都为 false。
/// 若某个 prev_cond = `A && x`，而当前 cond = `A && NOT(x)`，
/// 则在当前其它条件 A 成立且 prev_cond 已失败的语境下，NOT(x) 是冗余守卫，可剥除。
///
/// 例：
///   CASE
///     WHEN A && B THEN v1
///     WHEN A && NOT(B) THEN v2
///     WHEN true THEN v3
///   END
/// →
///   CASE
///     WHEN A && B THEN v1
///     WHEN A THEN v2
///     WHEN true THEN v3
///   END
fn is_not_conj_implied_by_prev_failure(
    cond: &Expr,
    current_conjuncts: &[Expr],
    prev_conds: &[&Expr],
) -> bool {
    let current_without_cond: Vec<&Expr> =
        current_conjuncts.iter().filter(|c| *c != cond).collect();

    prev_conds.iter().any(|prev| {
        let prev_conjuncts = flatten_and(prev);

        // 找到 prev_cond 中被当前 cond 反向互补的那个 conjunct。
        //
        // 旧形态：cond = NOT(B)，prev_conjunct = B
        // 新形态：NOT(comparison) 会先归一为反比较，如 cond = a <= b，prev_conjunct = a > b
        // 二者语义一致，都表示 prev_conjunct=false。
        let Some(complemented_prev) = prev_conjuncts.iter().find(|pc| is_not_of(cond, pc)) else {
            return false;
        };

        // prev_cond 除了 complemented_prev 以外的所有 conjunct，都必须已由当前其它 conjunct 保证。
        // 这样 prev_cond=false 才能推出 complemented_prev=false，即当前 cond=true。
        prev_conjuncts
            .iter()
            .all(|pc| pc == complemented_prev || current_without_cond.iter().any(|cc| *cc == pc))
    })
}

fn is_not_of(cond: &Expr, expected_inner: &Expr) -> bool {
    match cond.kind() {
        ExprKind::UnaryOp {
            op: UnaryOpKind::Not,
            operand,
        } => operand == expected_inner,
        _ => {
            cond.is_comparison_complement(expected_inner)
                || adjacent_integer_complement(cond, expected_inner)
        }
    }
}

/// Two closed adjacent bounds cover the exact integer domain, but not the
/// wider number domain. Require an acquired total integer explicitly.
fn adjacent_integer_complement(a: &Expr, b: &Expr) -> bool {
    use crate::BinOpKind::{Ge, Le};
    fn closed_bound(expr: &Expr) -> Option<(crate::BinOpKind, &Expr, num_bigint::BigInt)> {
        let (op, lhs, rhs) = expr.as_binop()?;
        match op {
            Ge | Le => {
                if let Some(bound) = rhs.as_lit().and_then(crate::Lit::as_integer) {
                    Some((op, lhs, bound))
                } else {
                    lhs.as_lit().and_then(crate::Lit::as_integer)
                        .map(|bound| (if op == Ge { Le } else { Ge }, rhs, bound))
                }
            }
            _ => None,
        }
    }
    let Some((op_a, subject_a, bound_a)) = closed_bound(a) else { return false };
    let Some((op_b, subject_b, bound_b)) = closed_bound(b) else { return false };
    if subject_a != subject_b || !super::integer_ac::total_integer(subject_a) {
        return false;
    }
    match (op_a, op_b) {
        (Le, Ge) => bound_a + 1 == bound_b,
        (Ge, Le) => bound_b + 1 == bound_a,
        _ => false,
    }
}

// Selecting a field of the first row under its existence guard, and a
// default on the empty path, is a single lazy FirstValue at the join.
fn select_first_default(expr: &Expr) -> Option<Expr> {
    use crate::domains::relation::{order_components, relation_exists, relation_first_value};
    let ([(guard, yes)], no) = expr.as_total_case_when()? else {
        return None;
    };
    for (value, default, invert) in [(yes, no, false), (no, yes, true)] {
        let Some((crate::RelationAggregateKind::FirstValue, ordered, Some(select), Some(_))) =
            value.as_relation_aggregate()
        else {
            continue;
        };
        let base = order_components(ordered).map_or(ordered, |(relation, _)| relation);
        let presence = relation_exists(base.clone());
        let test = if invert {
            crate::normalize(crate::builder::not(presence))
        } else {
            crate::normalize(presence)
        };
        if guard == &test {
            return Some(relation_first_value(
                ordered.clone(),
                select.clone(),
                default.clone(),
            ));
        }
    }
    None
}

#[cfg(test)]
mod first_match_domain_tests {
    use super::*;
    use crate::builder as b;

    fn flag(name: &str) -> Expr {
        b::param(name, crate::ScalarSymbolType::Boolean)
    }

    #[test]
    fn adjacent_closed_complements_are_integer_only_and_require_no_gap() {
        let n = b::param("n", crate::ScalarSymbolType::Integer);
        let unknown = b::param("n", crate::ScalarSymbolType::ExactDecimal);
        let complement = |n: Expr, high: i64| {
            adjacent_integer_complement(
                &b::le(n.clone(), b::lit_int(-1)),
                &b::ge(n, b::lit_int(high)),
            )
        };
        assert!(complement(n.clone(), 0));
        assert!(!complement(n, 1));
        assert!(!complement(unknown, 0));
    }

    #[test]
    fn null_partition_preserves_the_order_of_protected_evaluation() {
        let reference = b::heap_ref(-1);
        let nil = b::lit_null();
        let is_null = b::eq(reference.clone(), nil.clone());
        let not_null = b::ne(reference.clone(), nil);
        let extra = b::eq(reference, b::heap_ref(-2));
        let original = [
            (b::and(not_null.clone(), extra.clone()), b::lit_int(7)),
            (is_null.clone(), b::lit_int(11)),
            (b::lit_bool(true), b::lit_int(13)),
        ];
        assert_eq!(factor_leading_null_test(&original), Some(vec![
            original[1].clone(),
            (extra.clone(), b::lit_int(7)),
            original[2].clone(),
        ]));
        let reversed = [
            (b::and(extra, not_null), b::lit_int(7)),
            original[1].clone(),
            original[2].clone(),
        ];
        assert_eq!(factor_leading_null_test(&reversed), None);
    }

    #[test]
    fn partial_case_does_not_gain_a_normal_completion() {
        let p = flag("p");
        let partial = b::normal_return_case_when(vec![], (p, b::lit_int(7)));
        let never = b::normal_return_case_when(vec![], (b::lit_bool(false), b::lit_int(9)));

        for input in [partial, never] {
            assert_eq!(simplify(&input, &crate::DefaultSortContext), input);
            assert!(input.as_total_case_when().is_none());
        }
    }

    #[test]
    fn previous_guard_is_removed_only_inside_the_later_first_match_domain() {
        let p = flag("p");
        let q = flag("q");
        let branches = [
            (p.clone(), b::lit_int(10)),
            (b::or(p.clone(), q.clone()), b::lit_int(20)),
            (b::lit_bool(true), b::lit_int(30)),
        ];
        let (rewritten, changed) = strip_prev_disjuncts_from_or_guards(&branches);
        assert!(changed);
        assert_eq!(rewritten, vec![
            (p, b::lit_int(10)),
            (q, b::lit_int(20)),
            (b::lit_bool(true), b::lit_int(30)),
        ]);
    }

    #[test]
    fn a_failed_complement_only_removes_the_matching_later_conjunct() {
        let p = flag("p");
        let q = flag("q");
        let r = flag("r");
        let second_guard = b::and(p.clone(), q.clone());
        let input = |first| [
            (first, b::lit_int(10)),
            (second_guard.clone(), b::lit_int(20)),
            (b::lit_bool(true), b::lit_int(30)),
        ];
        let first = b::not(p);
        let (rewritten, changed) = simplify_branch_conditions_under_prior_failures(&input(first.clone()));
        assert!(changed);
        assert_eq!(rewritten, vec![
            (first, b::lit_int(10)),
            (q, b::lit_int(20)),
            (b::lit_bool(true), b::lit_int(30)),
        ]);

        let independent = input(r);
        let (unchanged, changed) = simplify_branch_conditions_under_prior_failures(&independent);
        assert!(!changed);
        assert_eq!(unchanged, independent.to_vec());
    }

    #[test]
    fn merging_reachable_payloads_keeps_total_fallback_even_when_masked() {
        let p = flag("p");
        let q = flag("q");
        let branches = [
            (b::and(p.clone(), q.clone()), b::lit_int(10)),
            (b::and(p.clone(), b::not(q)), b::lit_int(10)),
            (b::not(p.clone()), b::lit_int(20)),
            (b::lit_bool(true), b::lit_int(30)),
        ];
        let (grouped, changed) = group_same_value_effective_domains(&branches);
        assert!(changed);
        assert_eq!(grouped, vec![
            (p.clone(), b::lit_int(10)),
            (b::not(p), b::lit_int(20)),
            (b::lit_bool(true), b::lit_int(30)),
        ]);
    }
}

#[cfg(test)]
mod typed_branch_witness_tests {
    use super::*;
    use crate::{builder as b, ScalarSymbolType};

    #[test]
    fn integer_point_substitution_descends_only_through_exact_arithmetic() {
        let x = b::param("x", ScalarSymbolType::Integer);
        let point = b::lit_int(2);
        let general = b::add(b::mul(x.clone(), b::lit_int(2)), b::lit_int(3));
        let guard = b::eq(x, point);

        let case = |n| vec![(guard.clone(), b::lit_int(n)), (b::lit_bool(true), general.clone())];
        assert_eq!(redundant_integer_point_arm(&case(7)), Some(general.clone()));
        assert_eq!(redundant_integer_point_arm(&case(8)), None);
    }

    #[test]
    fn grouped_partial_case_still_has_no_explicit_completion_fallback() {
        let flag = |name| b::param(name, ScalarSymbolType::Boolean);
        let p = flag("p");
        let q = flag("q");
        let r = flag("r");
        let branches = [
            (b::and(p.clone(), q.clone()), b::lit_int(10)),
            (r, b::lit_int(20)),
            (b::and(p, b::not(q)), b::lit_int(10)),
        ];

        let (grouped, changed) = group_same_value_effective_domains(&branches);
        assert!(changed);
        assert_eq!(grouped.len(), 2);
        assert_eq!(grouped[0].1, b::lit_int(10));
        assert_eq!(grouped[1].1, b::lit_int(20));
        assert!(grouped.iter().all(|(guard, _)| guard.as_bool() != Some(true)));
    }

    #[test]
    fn first_value_fallback_requires_existence_of_the_same_relation() {
        use crate::domains::relation::{relation_exists, relation_first_value, RelationExpr};

        let offers = RelationExpr::base("offers", "offer");
        let unrelated = RelationExpr::base("bids", "bid");
        let row = b::row_value("offer");
        let first = relation_first_value(offers.clone(), row.clone(), b::lit_null());
        let fallback = b::lit_int(41);
        let conditional = |guard| b::case_when(vec![(guard, first.clone())], fallback.clone());

        assert_eq!(
            select_first_default(&conditional(relation_exists(offers.clone()))),
            Some(relation_first_value(offers, row, fallback.clone()))
        );
        assert_eq!(select_first_default(&conditional(relation_exists(unrelated))), None);
    }
}
