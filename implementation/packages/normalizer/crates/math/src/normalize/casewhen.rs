//! CaseWhen 化简规则。
//!
//! - 默认分支提前（第一个 condition 为 true 的分支即结果）
//! - 单分支退化：CaseWhen [(true, v)] → v
//! - 后续死分支消除：condition=false 的分支剔除
//! - 相邻互补 else 归一：CaseWhen [(c, v1), (NOT(c), v2)] / [(NOT(c), v1), (c, v2)] → [(first, v1), (true, v2)]
//! - 路径守卫冗余消除：branch[k].cond 里 AND 项匹配 NOT(branch[j].cond) (j<k) 剥除

use super::logical_chain::{flatten_and, flatten_or, rebuild_and, rebuild_or};
use crate::expr::{BinOpKind, Expr, ExprKind, Lit, UnaryOpKind};
use std::collections::HashSet;

pub(crate) fn simplify(expr: &Expr, ctx: &dyn crate::sort_context::SortContext) -> Expr {
    let Some(branches) = expr.as_case_when() else {
        return expr.clone();
    };
    if let Some(value) = select_first_default(expr) {
        return value;
    }
    let start = (branches.len() > 1 && crate::profile::enabled()).then(std::time::Instant::now);
    let (flatten_size, flatten_depth) = branches.iter().fold(
        (0usize, 0usize), |(size, depth), (condition, value)| {
            (size.saturating_add(condition.size()).saturating_add(value.size()),
             depth.max(value.depth()))
        },
    );
    let gate_skipped = flatten_size > 32_768 || flatten_depth > 16;
    let output = simplify_impl(expr, ctx, gate_skipped);
    if let Some(start) = start {
        let skip_reason = if gate_skipped {
            Some("flatten_cost_gate")
        } else if output == *expr {
            Some("no_rewrite")
        } else {
            None
        };
        let nested_values = branches.iter().filter(|(_, value)| value.as_case_when().is_some()).count();
        crate::profile::rule_event(
            "math.casewhen.group",
            expr.size(),
            output.size(),
            start.elapsed().as_micros(),
            skip_reason,
            &[
                ("branch_count", branches.len().to_string()),
                ("flatten_size", flatten_size.to_string()),
                ("flatten_depth", flatten_depth.to_string()),
                ("nested_values", nested_values.to_string()),
                ("flatten_gate", (!gate_skipped).to_string()),
            ],
        );
    }
    output
}

fn simplify_impl(expr: &Expr, ctx: &dyn crate::sort_context::SortContext, gate_skipped: bool) -> Expr {
    if let ExprKind::CaseWhen { branches } = expr.kind() {
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
        if gate_skipped {
            return expr.clone();
        }
        let (flattened_branches, flatten_changed) = flatten_nested_value_branches(branches);
        let branches_ref: &[(Expr, Expr)] = if flatten_changed {
            &flattened_branches
        } else {
            branches
        };

        if let Some(ranged) = super::integer_ranges::case_ranges(branches_ref) {
            // Normalize guards before comparison to avoid commuting AND terms
            // forever between this rule and the normal boolean ordering.
            let arms = ranged
                .as_case_when()
                .unwrap()
                .iter()
                .map(|(c, v)| (crate::normalize_with(c.clone(), ctx), v.clone()))
                .collect();
            // A certified complete integer partition is already in its
            // terminal domain form. Later prior-failure stripping must not
            // remove bounds that this partition would restore next iteration.
            return crate::builder::case_when(arms);
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

        let (dedup_by_cond, dedup_changed) =
            dead_branch_by_identical_prev_cond(&same_value_grouped);

        // 0d. 覆盖死分支消除：
        //    CaseWhen 按序匹配，branch[k] 只有在所有前序 cond 都失败时才会被检查。
        //    若 `cond[k] && !cond[0] && ... && !cond[k-1]` 经 normalize 可证明为 false，
        //    则 branch[k] 已被前序分支全集覆盖，永远不可达。
        //
        //    典型场景：外层 guard 已证明 `value != null`，而 return value 是同一个
        //    `value` 的 CaseWhen。nested flatten 会生成默认 null 分支；该分支被前面
        //    非 null value 分支覆盖，应删除，而不是渲染成 `"found:" + null`。
        let (covered_stripped, covered_changed) = dead_branch_by_covered_prev_conds(&dedup_by_cond);

        // 1. 先把相邻互补分支 canonicalize 成 else 分支：
        //    (c, v1), (NOT(c), v2) / (NOT(c), v1), (c, v2) → (first, v1), (true, v2)。
        //
        // 语义依据：CaseWhen 按序匹配。第二个分支只有在第一个分支为 false
        // 时才会被检查；若二者互补，第二个 cond 在该语境下必为 true，
        // 因此可归一为默认 else。
        // 只处理相邻互补，不做跨分支重排；比较反演 / De Morgan 互补由 is_not_of 证明。
        // Work from the original neighbor, not the rewritten guard. No
        // branch after the first true guard can affect the result.
        let mut canonicalized_changed = false;
        let mut filtered: Vec<(Expr, Expr)> = Vec::new();
        for (idx, (original, value)) in covered_stripped.iter().enumerate() {
            let complementary = idx > 0
                && (is_not_of(original, &covered_stripped[idx - 1].0)
                    || is_not_of(&covered_stripped[idx - 1].0, original));
            canonicalized_changed |= complementary;
            let condition = if complementary {
                crate::builder::lit_bool(true)
            } else {
                original.clone()
            };
            if condition.as_bool() == Some(false) {
                continue;
            }
            let is_default = condition.as_bool() == Some(true);
            filtered.push((condition, value.clone()));
            if is_default {
                break;
            }
        }

        // 2a. 在 first-match 有效域下投影布尔 branch value：
        //     CASE WHEN C THEN C WHEN true THEN X END -> CASE WHEN C THEN true WHEN true THEN X END
        //
        // 语义依据：CaseWhen branch[k] 被选中时，所有前序分支已失败且当前 cond 为真。
        // 若该有效域蕴含 value 为 true/false，则 value 可投影为对应布尔常量。
        // 这是控制域内的数学化简，不是 formatter/expected shape 特判。
        let (projected_values, projected_values_changed) =
            project_values_under_effective_domains(&filtered);
        let filtered = projected_values;

        // 3. 如果唯一分支是 (true, v)，退化为 v
        if filtered.len() == 1 {
            if let ExprKind::Lit(Lit::Bool(true)) = filtered[0].0.kind() {
                return filtered[0].1.clone();
            }
        }

        // 3a. 布尔 guard/value 恒等律：
        //    CASE WHEN !p THEN false WHEN true THEN p END -> p
        //    CASE WHEN p THEN true WHEN true THEN false END -> p
        //
        // 语义依据：cond 本身要求 p 是布尔表达式；CaseWhen first-match 后两分支
        // 正好覆盖 p 的真假域。此规则服务数学布尔收口，不是 formatter/display 特判。
        if let Some(collapsed) = collapse_two_branch_boolean_identity(&filtered) {
            return collapsed;
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
        //
        // 例：CaseWhen [(c, "x"), (true, "x")] → "x"
        //     CaseWhen [(c1, v), (c2, v), (true, v)] → v
        //
        // 语义依据：所有条件分支最终结果一致，条件本身不影响返回值。
        // 注意：这不等同于副作用合并——副作用由 push_side_effect 累积，此处只化简返回值。
        if filtered.len() >= 2 {
            let first_val = &filtered[0].1;
            if filtered.iter().all(|(_, v)| v == first_val) {
                let conds: Vec<Expr> = filtered.iter().map(|(cond, _)| cond.clone()).collect();
                if crate::builder::conditions_cover_all(&conds) {
                    return first_val.clone();
                }
            }
        }

        // 4. 如果 flatten / dedup / canonicalize / 过滤 / 截断 / 剥守卫 导致变化，返回新 CaseWhen
        if flatten_changed
            || guard_changed
            || or_changed
            || domain_simplified_changed
            || same_value_merged_changed
            || same_value_grouped_changed
            || dedup_changed
            || covered_changed
            || canonicalized_changed
            || projected_values_changed
            || filtered.len() != branches_ref.len()
            || filtered.len() != domain_simplified.len()
            || filtered.len() != same_value_grouped.len()
            || filtered.len() != covered_stripped.len()
        {
            if filtered.is_empty() {
                // 不应该发生（builder 保证至少有 true 分支），防御
                return expr.clone();
            }
            return Expr::new(ExprKind::CaseWhen { branches: filtered });
        }
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
    use crate::{builder as b, BinOpKind as B, Lit, UnaryOpKind};
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
        if let Some((UnaryOpKind::Neg, inner)) = value.as_unary_op() {
            return b::neg(at(inner, subject, point));
        }
        value.clone()
    }
    (crate::normalize(at(generic_value, subject, point)) == crate::normalize(at(point_value, subject, point)))
        .then(|| generic_value.clone())
}

fn collapse_two_value_exhaustive_to_default(branches: &[(Expr, Expr)]) -> Option<Expr> {
    let mut values: Vec<Expr> = Vec::new();
    let mut counts: Vec<usize> = Vec::new();
    for (_, value) in branches {
        if let Some(idx) = values.iter().position(|v| v == value) {
            counts[idx] += 1;
        } else {
            values.push(value.clone());
            counts.push(1);
        }
    }
    if values.len() != 2 {
        return None;
    }
    let effective_domains = first_match_effective_domains(branches);
    let raw_conds: Vec<Expr> = branches.iter().map(|(cond, _)| cond.clone()).collect();
    let raw_covered = crate::builder::conditions_cover_all(&raw_conds);
    let effective_covered = crate::builder::conditions_cover_all(&effective_domains);
    if !(raw_covered || effective_covered) {
        return None;
    }

    let (last_cond, last_value) = &branches[branches.len() - 1];
    let default_value = if last_cond.as_bool() == Some(true) {
        // An explicit first-match default remains only if its payload is not
        // less frequent than the other payload.
        let default_index = usize::from(last_value != &values[0]);
        if counts[default_index] < counts[1 - default_index] {
            return None;
        }
        last_value.clone()
    } else if counts[1] > counts[0] {
        values[1].clone()
    } else {
        // Equal counts without an explicit default choose the first payload.
        values[0].clone()
    };

    let mut out: Vec<(Expr, Expr)> = Vec::new();
    for ((cond, value), domain) in branches.iter().zip(effective_domains.into_iter()) {
        if value != &default_value {
            let keep_grouped_or = cond.as_bool() != Some(true) && flatten_or(cond).len() > 1;
            let displayed_domain = crate::normalize(domain);
            if keep_grouped_or {
                // Keep merged alternatives grouped to avoid split/merge
                // oscillation, but only after restoring their semantic domain.
                out.push((displayed_domain, value.clone()));
            } else {
                for disjunct in flatten_or(&displayed_domain) {
                    out.push((disjunct, value.clone()));
                }
            }
        }
    }
    let expands_grouped_binary_partition = branches.len() == 2
        && out.len() == 2
        && branches.iter().any(|(cond, _)| flatten_or(cond).len() > 1);
    if out.len() + 1 > branches.len() && !expands_grouped_binary_partition {
        return None;
    }
    out.push((crate::builder::lit_bool(true), default_value));
    if out == branches {
        return None;
    }
    Some(Expr::new(ExprKind::CaseWhen { branches: out }))
}

fn project_values_under_effective_domains(branches: &[(Expr, Expr)]) -> (Vec<(Expr, Expr)>, bool) {
    let domains = first_match_effective_domains(branches);
    let mut changed = false;
    let mut out = Vec::with_capacity(branches.len());
    for ((cond, value), effective) in branches.iter().zip(&domains) {
        let projected = project_value_under_domain(value, effective);
        changed |= projected != *value;
        // Availability of a row in the selected value does not prove its guard
        // before this branch is taken. Keep the guard and its acquisition domain.
        out.push((cond.clone(), projected));
    }
    (out, changed)
}

pub(super) fn project_value_under_domain(value: &Expr, domain: &Expr) -> Expr {
    if domain.as_bool() == Some(true) || is_projectable_boolean_value(value) {
        return project_boolean_condition_under_domain(value, domain);
    }

    // Guard-equality value substitution:
    // If domain AND-conjuncts contain `value == literal` (or `literal == value`),
    // the value is provably equal to that literal under this domain.
    // Example: WHEN ... && $t_1 == 0 THEN $t_1  →  THEN 0
    if value.as_lit().is_none() {
        let conjuncts = flatten_and(domain);
        for conj in &conjuncts {
            if let ExprKind::BinOp {
                op: BinOpKind::Eq,
                left,
                right,
            } = conj.kind()
            {
                let replacement = if left == value {
                    right
                } else if right == value {
                    left
                } else {
                    continue;
                };
                if replacement.as_lit().is_some() {
                    return replacement.clone();
                }
            }
        }
    }

    match value.kind() {
        ExprKind::BinOp {
            op,
            left: _,
            right: _,
        } => match op {
            BinOpKind::And
            | BinOpKind::Or
            | BinOpKind::Eq
            | BinOpKind::Ne
            | BinOpKind::Lt
            | BinOpKind::Le
            | BinOpKind::Gt
            | BinOpKind::Ge => project_boolean_condition_under_domain(value, domain),
            _ => value.clone(),
        },
        ExprKind::UnaryOp {
            op: UnaryOpKind::Not,
            ..
        } => project_boolean_condition_under_domain(value, domain),
        ExprKind::CaseWhen { branches } =>
            project_case_under_domain(value, branches, domain, project_value_under_domain),
        // A host branch is not proof about values bound inside a relation.
        // Relation projection has its own same-row proof.
        _ => value.clone(),
    }
}

fn is_projectable_boolean_value(expr: &Expr) -> bool {
    if super::boolean_casewhen::is_boolean_expr(expr) {
        return true;
    }
    match expr.kind() {
        ExprKind::SymVar { origin } => match origin {
            crate::symvar::SymVarOrigin::Parameter { primitive_type, .. }
            | crate::symvar::SymVarOrigin::External { primitive_type, .. }
            | crate::symvar::SymVarOrigin::Temporal { primitive_type, .. } => {
                primitive_type == "boolean"
            }
        },
        _ => false,
    }
}

fn project_boolean_condition_under_domain(cond: &Expr, domain: &Expr) -> Expr {
    if cond.as_bool().is_some() {
        return cond.clone();
    }
    if crate::builder::path_implies(domain, cond) {
        return crate::builder::lit_bool(true);
    }
    if crate::builder::path_implies(domain, &crate::builder::not(cond.clone())) {
        return crate::builder::lit_bool(false);
    }

    match cond.kind() {
        ExprKind::BinOp { op: op @ (BinOpKind::And | BinOpKind::Or), left, right } => {
            let left = project_boolean_condition_under_domain(left, domain);
            let and = *op == BinOpKind::And;
            let entrance = if and { left.clone() } else { crate::builder::not(left.clone()) };
            let scoped = crate::normalize(crate::builder::and(domain.clone(), entrance));
            let right = project_boolean_condition_under_domain(right, &scoped);
            crate::normalize(if and {
                crate::builder::and(left, right)
            } else {
                crate::builder::or(left, right)
            })
        }
        ExprKind::UnaryOp {
            op: UnaryOpKind::Not,
            operand,
        } => {
            let projected = project_boolean_condition_under_domain(operand, domain);
            crate::normalize(crate::builder::not(projected))
        }
        ExprKind::CaseWhen { branches } =>
            project_case_under_domain(cond, branches, domain, project_boolean_condition_under_domain),
        _ => cond.clone(),
    }
}

/// Scope each original first-match arm before projecting its typed payload.
fn project_case_under_domain(
    original: &Expr,
    branches: &[(Expr, Expr)],
    domain: &Expr,
    project_payload: fn(&Expr, &Expr) -> Expr,
) -> Expr {
    let nested_domains = first_match_effective_domains(branches);
    let mut changed = false;
    let mut out = Vec::with_capacity(branches.len());
    for ((cond, value), nested_effective) in branches.iter().zip(nested_domains.iter()) {
        let combined_domain = crate::normalize(crate::builder::and(
            domain.clone(),
            nested_effective.clone(),
        ));
        let projected_cond = project_boolean_condition_under_domain(cond, domain);
        let projected_value = project_payload(value, &combined_domain);
        changed |= projected_cond != *cond || projected_value != *value;
        out.push((projected_cond, projected_value));
    }
    if changed {
        Expr::new(ExprKind::CaseWhen { branches: out })
    } else {
        original.clone()
    }
}

fn collapse_two_branch_boolean_identity(branches: &[(Expr, Expr)]) -> Option<Expr> {
    if branches.len() != 2 {
        return None;
    }
    let (first_cond, first_value) = &branches[0];
    let (second_cond, second_value) = &branches[1];
    if second_cond.as_bool() != Some(true) {
        return None;
    }

    let first_is_false = first_value.as_bool() == Some(false);
    let second_is_false = second_value.as_bool() == Some(false);
    let second_is_true = second_value.as_bool() == Some(true);

    if first_is_false && is_not_of(first_cond, second_value) {
        return Some(second_value.clone());
    }
    // A reducer may represent a total Boolean observation as
    // `CASE WHEN p THEN p ELSE false END`.  Preserve the observation itself
    // instead of retaining an eager-looking guard around a lazy relation
    // aggregate.  The identity is valid only for total Boolean predicates.
    if first_cond == first_value
        && second_is_false
        && super::boolean::total_boolean_value(first_cond)
    {
        return Some(first_cond.clone());
    }
    if first_is_false && second_is_true && super::boolean::total_boolean_value(first_cond) {
        return Some(crate::builder::not(first_cond.clone()));
    }
    None
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
            if inner_branches
                .last()
                .is_none_or(|(cond, _)| cond.as_bool() != Some(true))
            {
                // A partial value CASE has an implicit null alternative. It
                // belongs to this selected outer branch: without this arm an
                // unmatched inner CASE would fall through to a later outer
                // value. Never treat its last explicit condition as true.
                out.push((outer_cond.clone(), crate::builder::lit_null()));
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
            .filter(|d| !is_covered_by_previous_or(d, &previous_or))
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

    // The explicit-default extension is only for partitions with at least
    // three reachable payloads. Count effective groups, not raw values: dead
    // arms must not divert an actually binary partition from its existing rule.
    if explicit_default.is_some() && groups.len() < 3 {
        return (branches.to_vec(), false);
    }

    if groups.len() >= branches.len() || groups.iter().all(|(_, domains)| domains.len() == 1) {
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

/// 死分支消除（相同 cond 的后续 branch）：若 branch[k].cond 与某 branch[j].cond (j<k)
/// 完全相等，则 branch[k] 永远不会被匹配（前面的必先命中），剔除。
///
/// 注意：必须保证最后一个 true 兜底分支不被误删（builder::case_when 约束）。
fn dead_branch_by_identical_prev_cond(branches: &[(Expr, Expr)]) -> (Vec<(Expr, Expr)>, bool) {
    let mut out: Vec<(Expr, Expr)> = Vec::with_capacity(branches.len());
    let mut changed = false;

    for (cond, value) in branches.iter() {
        let dup = out.iter().any(|(prev_cond, _)| prev_cond == cond);
        if dup {
            changed = true;
            continue;
        }
        out.push((cond.clone(), value.clone()));
    }

    (out, changed)
}

fn dead_branch_by_covered_prev_conds(branches: &[(Expr, Expr)]) -> (Vec<(Expr, Expr)>, bool) {
    let mut out: Vec<(Expr, Expr)> = Vec::with_capacity(branches.len());
    let mut changed = false;

    for (cond, value) in branches.iter() {
        if is_covered_by_previous(cond, &out) {
            changed = true;
            continue;
        }
        out.push((cond.clone(), value.clone()));
    }

    (out, changed)
}

fn is_covered_by_previous(cond: &Expr, previous: &[(Expr, Expr)]) -> bool {
    if previous.is_empty() {
        return false;
    }

    let previous_conds: Vec<Expr> = previous
        .iter()
        .map(|(prev_cond, _)| prev_cond.clone())
        .collect();

    if cond.as_bool() == Some(true) {
        return crate::builder::conditions_cover_all(&previous_conds);
    }

    let previous_or = previous_conds
        .into_iter()
        .reduce(crate::builder::or)
        .unwrap_or_else(|| crate::builder::lit_bool(false));
    crate::builder::path_implies(cond, &previous_or)
}

fn is_covered_by_previous_or(cond: &Expr, previous_or: &Expr) -> bool {
    crate::builder::path_implies(cond, previous_or)
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
                    && !is_demorgan_conj_implied_by_prev_failure(c, &conjuncts, prev_conds)
            })
            .cloned()
            .collect();
        (filtered.len() != original_len).then(|| rebuild_and(&filtered))
    })
}

/// 检查 cond 是否是 NOT(expected) 对任一 prev_cond 成立。
fn is_not_of_any(cond: &Expr, prev_conds: &[&Expr]) -> bool {
    for prev in prev_conds {
        if is_not_of(cond, prev) {
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

/// 检查当前分支中的某个 conjunct 是否属于“前序 OR 分支失败”推出的 De Morgan 补集。
///
/// CaseWhen 按序匹配：进入 branch[k] 时，所有 prev_cond 都为 false。
/// 若某个 prev_cond = `A || B`，而当前 cond = `NOT(A) && NOT(B) && C`，
/// 则 `NOT(A) && NOT(B)` 只是“前序分支失败”的展开形式，在当前分支上冗余，
/// 可剥除为 `C`。
///
/// 这不是排序/合并 CaseWhen arms，也不是隐藏 default；只是把 first-match
/// 语义中已经隐含的“前序 OR 失败”从后续分支 guard 里删除。
fn is_demorgan_conj_implied_by_prev_failure(
    cond: &Expr,
    current_conjuncts: &[Expr],
    prev_conds: &[&Expr],
) -> bool {
    prev_conds.iter().any(|prev| {
        let prev_disjuncts = flatten_or(prev);
        if prev_disjuncts.len() <= 1 {
            return false;
        }

        let mut used = vec![false; current_conjuncts.len()];
        let mut contains = false;

        for prev_disjunct in &prev_disjuncts {
            let Some(idx) = current_conjuncts
                .iter()
                .enumerate()
                .find_map(|(idx, current)| {
                    (!used[idx] && is_not_of(current, prev_disjunct)).then_some(idx)
                })
            else {
                return false;
            };
            used[idx] = true;
            contains |= &current_conjuncts[idx] == cond;
        }

        contains
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
        }
    }
}

// Selecting a field of the first row under its existence guard, and a
// default on the empty path, is a single lazy FirstValue at the join.
fn select_first_default(expr: &Expr) -> Option<Expr> {
    use crate::domains::relation::{order_components, relation_exists, relation_first_value};
    let [(guard, yes), (tail, no)] = expr.as_case_when()? else {
        return None;
    };
    if tail.as_bool() != Some(true) {
        return None;
    }
    for (value, default, invert) in [(yes, no, false), (no, yes, true)] {
        let Some((crate::RelationAggregateKind::FirstValue, ordered, Some(select), Some(null))) =
            value.as_relation_aggregate()
        else {
            continue;
        };
        if !null.is_null() {
            continue;
        }
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
