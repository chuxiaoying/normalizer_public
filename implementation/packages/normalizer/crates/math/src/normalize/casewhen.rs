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

pub(crate) fn simplify(
    expr: &Expr,
    ctx: &dyn crate::sort_context::SortContext,
    parent: Option<&Expr>,
) -> Expr {
    let Some(branches) = expr.as_case_when() else {
        return expr.clone();
    };
    // A WHEN that returns SQL UNKNOWN does not select its arm, but its
    // negation is UNKNOWN as well. A two-valued first-match proof cannot
    // invent either an exhaustive complement or facts for following arms.
    if branches
        .iter()
        .any(|(guard, _)| guard.may_observe_sql_unknown())
    {
        let mut unchanged_when = Vec::new();
        for (guard, value) in branches {
            if guard.as_bool() == Some(false) {
                continue;
            }
            unchanged_when.push((guard.clone(), value.clone()));
            if guard.as_bool() == Some(true) {
                break;
            }
        }
        if unchanged_when.len() != branches.len() {
            return Expr::new(ExprKind::CaseWhen {
                branches: unchanged_when,
            });
        }
        return expr.clone();
    }
    if let Some(value) = crate::case_orientation::canonicalize_terminal_completion(expr, ctx) {
        return value;
    }
    if let Some(value) = crate::case_orientation::canonicalize_terminal_extremum(expr, ctx) {
        return value;
    }
    if let Some(value) = select_first_default(expr) {
        return value;
    }
    let start = (branches.len() > 1 && crate::profile::enabled()).then(std::time::Instant::now);
    let output = simplify_impl(expr, branches, ctx, parent);
    if let Some(start) = start {
        let skip_reason = (output == *expr).then_some("no_rewrite");
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
    for (index, ((protected, selected), (null, on_null))) in
        branches.iter().zip(branches.iter().skip(1)).enumerate()
    {
        let Some((Eq, subject, nil)) = null.as_binop() else {
            continue;
        };
        if subject.as_heap_ref().is_none() || !nil.is_null() {
            continue;
        }
        if !matches!(protected.as_binop(), Some((crate::BinOpKind::And, ..))) {
            continue;
        }
        // A binary conjunction contributes at least two leaves.
        let parts = flatten_and(protected);
        let (first, rest) = (&parts[0], &parts[1..]);
        if !matches!(first.as_binop(), Some((Ne, other, rhs)) if other == subject && rhs == nil) {
            continue;
        }
        let mut result = branches.to_vec();
        result[index] = (null.clone(), on_null.clone());
        result[index + 1] = (rebuild_and(rest), selected.clone());
        return Some(result);
    }
    None
}

/// Match the Boolean value of a complete test on either side of one
/// already acquired reference's null partition.
fn null_boolean_arms<'a>(condition: &'a Expr, is_null: &Expr) -> Option<(&'a Expr, &'a Expr)> {
    let ([(test, yes)], no) = condition.as_total_case_when()? else {
        return None;
    };
    if test == is_null {
        Some((yes, no))
    } else if test.is_comparison_complement(is_null) {
        Some((no, yes))
    } else {
        None
    }
}

fn restrict_nonnull_guard(guard: &Expr, is_null: &Expr) -> Expr {
    use crate::{
        builder as b,
        BinOpKind::{And, Or},
    };
    if guard == is_null {
        return b::lit_bool(false);
    }
    if guard.is_comparison_complement(is_null) {
        return b::lit_bool(true);
    }
    if let Some((_, on_nonnull)) = null_boolean_arms(guard, is_null) {
        return restrict_nonnull_guard(on_nonnull, is_null);
    }
    if let Some((op @ (And | Or), left, right)) = guard.as_binop() {
        return b::binop_public(
            op,
            restrict_nonnull_guard(left, is_null),
            restrict_nonnull_guard(right, is_null),
        );
    }
    if let Some(operand) = guard.as_not() {
        return b::not(restrict_nonnull_guard(operand, is_null));
    }
    guard.clone()
}

/// A short-circuit CASE that is false on null cannot preempt the next
/// unconditional null arm. Make that null arm first, then specialize
/// only the remaining guards, where the reference is already non-null.
fn factor_short_circuit_null_test(branches: &[(Expr, Expr)]) -> Option<Vec<(Expr, Expr)>> {
    use crate::BinOpKind::Eq;
    for (index, ((protected, selected), (nullable, on_null))) in
        branches.iter().zip(branches.iter().skip(1)).enumerate()
    {
        let clauses = flatten_and(nullable);
        let Some(is_null) = clauses.iter().find(|term| matches!(term.as_binop(), Some((Eq, a, z))
            if (a.as_heap_ref().is_some() && z.is_null()) || (z.as_heap_ref().is_some() && a.is_null())))
        else { continue; };
        let Some((when_null, when_nonnull)) = null_boolean_arms(protected, is_null) else {
            continue;
        };
        if when_null.as_bool() != Some(false)
            || !clauses.iter().all(|term| {
                term == is_null
                    || null_boolean_arms(term, is_null)
                        .is_some_and(|(arm, _)| arm.as_bool() == Some(true))
            })
        {
            continue;
        }
        let mut result = branches[..index].to_vec();
        result.push((is_null.clone(), on_null.clone()));
        result.push((
            restrict_nonnull_guard(when_nonnull, is_null),
            selected.clone(),
        ));
        result.extend(branches[index + 2..].iter().map(|(guard, value)| {
            (
                crate::normalize(restrict_nonnull_guard(guard, is_null)),
                value.clone(),
            )
        }));
        return Some(result);
    }
    None
}

/// A shared final non-null guard is a partition of the whole CASE when
/// every preceding total predicate already requires that same non-null
/// reference. Only then may the null branch become the first test.
fn factor_total_nonnull_tail(expr: &Expr, ctx: &dyn crate::SortContext) -> Option<Expr> {
    use crate::BinOpKind::{Eq, Ne};
    let (branches, on_null) = expr.as_total_case_when()?;
    let ((nonnull, when_nonnull), earlier) = branches.split_last()?;
    if earlier.is_empty() {
        return None;
    }
    let (Ne, lhs, rhs) = nonnull.as_binop()? else {
        return None;
    };
    let reference = if lhs.is_null() {
        rhs
    } else if rhs.is_null() {
        lhs
    } else {
        return None;
    };
    if !super::boolean_casewhen::total_value_with_context(reference, ctx)
        || reference.as_heap_ref().is_none()
    {
        return None;
    }
    let null = crate::builder::binop_public(Eq, lhs.clone(), rhs.clone());
    let mut parts = vec![(null, on_null.clone())];
    for (guard, value) in earlier {
        if !super::boolean_casewhen::total_value_with_context(guard, ctx) {
            return None;
        }
        let conjunction = flatten_and(guard);
        if !conjunction.contains(nonnull) {
            return None;
        }
        let remaining: Vec<_> = conjunction
            .into_iter()
            .filter(|term| term != nonnull)
            .collect();
        parts.push((rebuild_and(&remaining), value.clone()));
    }
    let candidate = crate::builder::case_when(parts, when_nonnull.clone());
    crate::canonical_order_compare(&candidate, expr, ctx)
        .is_lt()
        .then_some(candidate)
}

fn simplify_impl(
    expr: &Expr,
    branches: &[(Expr, Expr)],
    ctx: &dyn crate::sort_context::SortContext,
    parent: Option<&Expr>,
) -> Expr {
    if let Some(value) = crate::domains::text::redundant_null_default(expr) {
        return value;
    }
    if let Some(value) = redundant_integer_point_arm(branches) {
        return value;
    }
    if let Some(value) = redundant_cardinality_arm(expr) {
        return value;
    }
    // An exhaustive binary partition may be complemented only together
    // with swapping its values. Use the shared predicate ordering so source
    // branch order cannot choose two different normal forms.
    if let Some(oriented) = crate::case_orientation::canonicalize_binary_case(expr, ctx, parent) {
        return oriented;
    }

    // For an acquired Boolean selector, writing its complement first
    // also preserves the inner CASE's original lazy evaluation domain.
    // Consider this whole-tree candidate before repeating the selector in
    // every flattened arm.
    if let Some(candidate) = flatten_selected_case(expr, ctx) {
        return candidate;
    }
    if let Some(candidate) = factor_exhaustive_selector_tail(expr, ctx) {
        return candidate;
    }

    // A complete nested value CASE can remove one condition arm when
    // flattened. Keep each inner guard behind the original outer guard so
    // the flatten does not read values from an unselected branch.
    let (flattened_branches, flatten_changed) = flatten_nested_value_branches(branches);
    let branches_ref: &[(Expr, Expr)] = if flatten_changed {
        &flattened_branches
    } else {
        branches
    };

    if let Some(guarded) = factor_short_circuit_null_test(branches_ref)
        .or_else(|| factor_leading_null_test(branches_ref))
    {
        return Expr::new(ExprKind::CaseWhen { branches: guarded });
    }
    if let Some(guarded) = factor_total_nonnull_tail(expr, ctx) {
        return guarded;
    }

    if let Some((ranged, fallback)) = super::integer_ranges::case_ranges(branches_ref) {
        // Normalize guards before comparison to avoid commuting AND terms
        // forever between this rule and the normal boolean ordering.
        let mut remaining = parent.cloned();
        let arms: Vec<_> = ranged
            .into_iter()
            .map(|(guard, value)| {
                let guard = crate::normalize_with(guard, ctx);
                let representative =
                    super::integer_ranges::canonical_guard(&guard, remaining.as_ref(), ctx)
                        .map(|candidate| crate::normalize_with(candidate, ctx))
                        .unwrap_or_else(|| guard.clone());
                if representative.as_bool() != Some(false) {
                    let next = crate::builder::not(guard);
                    remaining = Some(match remaining.take() {
                        Some(previous) => crate::builder::and(previous, next),
                        None => next,
                    });
                }
                (representative, value)
            })
            .filter(|(guard, _)| guard.as_bool() != Some(false))
            .collect();
        // The range calculation proves each effective domain, but an
        // ordered CASE only needs to state what prior failures do not
        // already imply. Run the same projection inside this rule so a
        // later range pass cannot restore and publish the redundant bound.
        let (guards, _) = strip_redundant_path_guards(&arms);
        let ranged = crate::builder::case_when(guards, fallback);
        return ranged
            .as_case_when()
            .and_then(|branches| canonical_disjoint_literal_arms(branches, ctx))
            .unwrap_or(ranged);
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
    let (covered_stripped, covered_changed) =
        dead_branch_by_covered_prev_conds(&same_value_grouped);

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

    if let Some(rewritten) = orient_adjacent_integer_suffix(&filtered, ctx) {
        return rewritten;
    }
    if let Some(((last_guard, fallback), prior)) = filtered.split_last() {
        if last_guard.as_bool() != Some(true)
            && super::integer_ranges::exhaustive_acquired_suffix(&filtered)
        {
            return crate::builder::case_when(prior.to_vec(), fallback.clone());
        }
    }

    // Try the two-value first-match form without splitting a disjunction
    // into several copies of the same result arm.
    if let Some(collapsed) = collapse_two_value_exhaustive_to_default(&filtered, ctx) {
        return collapsed;
    }

    if let Some(reordered) = canonical_disjoint_literal_arms(&filtered, ctx) {
        return reordered;
    }

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

/// Distinct literal equalities against one total, bound parameter cannot
/// overlap within either of the checked exact domains below. The selector
/// is already acquired and evaluating these terms adds no observations.
fn canonical_disjoint_literal_arms(
    branches: &[(Expr, Expr)],
    ctx: &dyn crate::sort_context::SortContext,
) -> Option<Expr> {
    use crate::{BinOpKind::Eq, SymVarOrigin};
    let ((fallback_guard, fallback), arms) = branches.split_last()?;
    if arms.len() < 2 || fallback_guard.as_bool() != Some(true) || fallback.as_lit().is_none() {
        return None;
    }
    let mut selector: Option<&Expr> = None;
    #[derive(Eq, Hash, PartialEq)]
    enum Label {
        Text(Vec<u16>),
        Integer(num_bigint::BigInt),
    }

    let mut labels = HashSet::new();
    for (guard, result) in arms {
        result.as_lit()?;
        let (Eq, left, right) = guard.as_binop()? else {
            return None;
        };
        let (subject, literal) = if let Some(literal) = right.as_lit() {
            (left, literal)
        } else {
            (right, left.as_lit()?)
        };
        let Some(SymVarOrigin::Parameter { primitive_type, .. }) = subject.as_sym_var() else {
            return None;
        };
        let label = match primitive_type.as_str() {
            "text" => Label::Text(literal.text_units()?),
            "integer" => Label::Integer(literal.as_integer()?),
            _ => return None,
        };
        if selector.is_some_and(|previous| previous != subject) || !labels.insert(label) {
            return None;
        }
        selector = Some(subject);
    }
    let mut sorted = arms.to_vec();
    sorted.sort_by(|(a, _), (b, _)| crate::canonical_order_compare(a, b, ctx));
    (sorted != arms).then(|| crate::builder::case_when(sorted, fallback.clone()))
}

/// On the remaining domain, the last two strict tests of exactly the
/// same acquired integer are an exhaustive binary CASE. Keep the whole
/// suffix behind every earlier arm (notably a null/definedness check).
fn orient_adjacent_integer_suffix(
    branches: &[(Expr, Expr)],
    ctx: &dyn crate::sort_context::SortContext,
) -> Option<Expr> {
    let [prefix @ .., (yes_guard, yes), (no_guard, no)] = branches else {
        return None;
    };
    use super::integer_ranges::{acquired_integer_strict_bound, adjacent_acquired_integer_bounds};
    if acquired_integer_strict_bound(yes_guard).is_none()
        || no_guard.as_bool() != Some(true)
            && !adjacent_acquired_integer_bounds(yes_guard, no_guard)
    {
        return None;
    }
    let pair = crate::builder::case_when(vec![(yes_guard.clone(), yes.clone())], no.clone());
    let pair = crate::case_orientation::canonicalize_binary_case(&pair, ctx, None).unwrap_or(pair);
    let ([(guard, selected)], fallback) = pair.as_total_case_when()? else {
        return None;
    };
    if no_guard.as_bool() == Some(true) && guard == yes_guard && selected == yes && fallback == no {
        return None;
    }
    let mut arms = prefix.to_vec();
    arms.push((guard.clone(), selected.clone()));
    Some(crate::builder::case_when(arms, fallback.clone()))
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
    let [(condition, yes), (always, no)] = branches else {
        return None;
    };
    if always.as_bool() != Some(true) {
        return None;
    }
    let (op, left, right) = condition.as_binop()?;
    let (point_value, generic_value) = match op {
        B::Eq => (yes, no),
        B::Ne => (no, yes),
        _ => return None,
    };
    let is_integer = |value: &Expr| value.as_lit().and_then(Lit::as_integer).is_some();
    let (subject, point) = if is_integer(right) {
        (left, right)
    } else {
        (right, left)
    };
    if subject.as_sym_var().is_none()
        || !is_integer(point)
        || !super::integer_ac::total_integer(subject)
        || !super::integer_ac::total_integer(generic_value)
        || !super::integer_ac::total_integer(point_value)
    {
        return None;
    }
    fn at(value: &Expr, subject: &Expr, point: &Expr) -> Expr {
        if value == subject {
            return point.clone();
        }
        if let Some((op, left, right)) = value.as_binop() {
            if matches!(op, B::Add | B::Sub | B::Mul) {
                return b::binop_public(op, at(left, subject, point), at(right, subject, point));
            }
        }
        value.clone()
    }
    (crate::normalize(at(generic_value, subject, point))
        == crate::normalize(at(point_value, subject, point)))
    .then(|| generic_value.clone())
}

// A comparison of a bare Count is total on the same acquired snapshot
// used by its empty test. Reuse the OR absorption without moving any
// acquisition, filtered projection or unrelated nullable read.
fn redundant_cardinality_arm(expr: &Expr) -> Option<Expr> {
    let ([(guard, yes)], no) = expr.as_total_case_when()? else {
        return None;
    };
    if let (Some(yes), Some(no)) = (yes.as_bool(), no.as_bool()) {
        if yes != no && super::relation::relation_count_bound(guard).is_some() {
            return Some(if yes {
                guard.clone()
            } else {
                crate::builder::not(guard.clone())
            });
        }
    }
    if yes.as_bool() == Some(true) {
        return super::boolean::absorbed_cardinality_disjunction(guard, no);
    }
    if no.as_bool() == Some(true) {
        let opposite = crate::builder::not(guard.clone());
        return super::boolean::absorbed_cardinality_disjunction(&opposite, yes);
    }
    None
}

fn collapse_two_value_exhaustive_to_default(
    branches: &[(Expr, Expr)],
    ctx: &dyn crate::sort_context::SortContext,
) -> Option<Expr> {
    if branches.len() < 3 {
        return None;
    }
    let (last_cond, default_value) = branches.last()?;
    if last_cond.as_bool() != Some(true)
        || !branches
            .iter()
            .all(|(guard, _)| super::boolean_casewhen::total_value(guard))
    {
        return None;
    }
    let mut values = branches.iter().map(|(_, value)| value);
    let distinct = values.find(|value| *value != default_value)?;
    if values.any(|value| value != default_value && value != distinct) {
        return None;
    }
    // This CASE is complete. Union the effective domains for the non-default
    // value; never use the raw guards across different earlier payloads.
    let effective = first_match_effective_domains(branches);
    let guard = branches
        .iter()
        .zip(effective)
        .filter_map(|((_, value), domain)| (value == distinct).then_some(domain))
        .reduce(crate::builder::or)?;
    let guard = crate::normalize_with(guard, ctx);
    let candidate =
        crate::builder::case_when(vec![(guard, distinct.clone())], default_value.clone());
    let original = Expr::new(ExprKind::CaseWhen {
        branches: branches.to_vec(),
    });
    crate::canonical_order_compare(&candidate, &original, ctx)
        .is_lt()
        .then_some(candidate)
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

fn flatten_selected_case(expr: &Expr, ctx: &dyn crate::SortContext) -> Option<Expr> {
    let ([(outer, selected)], skipped) = expr.as_total_case_when()? else {
        return None;
    };
    let selector = outer.as_not().unwrap_or(outer);
    if !matches!(selector.as_sym_var(), Some(crate::SymVarOrigin::Parameter { name, primitive_type })
        if primitive_type == "boolean" && !name.starts_with('$') && !name.contains(['.', '[', ']']))
        || skipped.as_case_when().is_some()
    {
        return None;
    }
    let (inside, fallback) = selected.as_total_case_when()?;
    let first = crate::normalize_with(crate::builder::not(outer.clone()), ctx);
    let mut parts = vec![(first, skipped.clone())];
    for (guard, value) in inside {
        // Only the actually proved outer selector can be removed; no
        // other nested observation is moved or removed.
        let remaining: Vec<_> = flatten_and(guard)
            .into_iter()
            .filter(|test| test != outer)
            .collect();
        let guard = rebuild_and(&remaining);
        parts.push((guard, value.clone()));
    }
    let candidate = crate::builder::case_when(parts, fallback.clone());
    crate::canonical_order_compare(&candidate, expr, ctx)
        .is_lt()
        .then_some(candidate)
}

/// Execution can produce a flat list of guarded returns instead of a
/// nested CASE. Recover the same partition only if the inner outcomes
/// cover the selected domain; a partial expression never gains an ELSE.
fn factor_exhaustive_selector_tail(expr: &Expr, ctx: &dyn crate::SortContext) -> Option<Expr> {
    let (last, earlier) = expr.as_case_when()?.split_last()?;
    if earlier.len() < 2 {
        return None;
    }
    let selector = last.0.as_not()?;
    if !matches!(selector.as_sym_var(), Some(crate::SymVarOrigin::Parameter { name, primitive_type })
        if primitive_type == "boolean" && !name.starts_with('$') && !name.contains(['.', '[', ']']))
    {
        return None;
    }
    let mut inner = Vec::with_capacity(earlier.len());
    for (guard, value) in earlier {
        let terms = flatten_and(guard);
        if !terms.contains(selector) {
            return None;
        }
        let terms: Vec<_> = terms.into_iter().filter(|test| test != selector).collect();
        let remaining = rebuild_and(&terms);
        if remaining.as_bool().is_none()
            && !super::boolean_domains::is_closed_boolean(&remaining)
            && !super::integer_ranges::is_closed_integer_guard(&remaining)
        {
            return None;
        }
        inner.push((remaining, value.clone()));
    }
    let guards: Vec<_> = inner.iter().map(|(guard, _)| guard.clone()).collect();
    if !crate::builder::conditions_cover_all(&guards) {
        return None;
    }
    let (_, fallback) = inner.pop()?;
    let mut parts = vec![last.clone()];
    parts.extend(inner);
    let candidate = crate::builder::case_when(parts, fallback);
    crate::canonical_order_compare(&candidate, expr, ctx)
        .is_lt()
        .then_some(candidate)
}

fn flatten_nested_value_branches(branches: &[(Expr, Expr)]) -> (Vec<(Expr, Expr)>, bool) {
    let mut out: Vec<(Expr, Expr)> = Vec::with_capacity(branches.len());
    let mut changed = false;

    for (outer_cond, value) in branches.iter() {
        // If the inner CASE has no ELSE, exhausting its guards ends
        // in an undefined inner value; it does not resume the outer CASE.
        // Flatten only the explicit total form where no such fallthrough
        // can enter an outer sibling.
        let Some((inner_branches, fallback)) = value.as_total_case_when() else {
            out.push((outer_cond.clone(), value.clone()));
            continue;
        };
        changed = true;
        for (inner_cond, inner_val) in inner_branches.iter() {
            out.push((
                crate::builder::and(outer_cond.clone(), inner_cond.clone()),
                inner_val.clone(),
            ));
        }
        out.push((outer_cond.clone(), fallback.clone()));
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
        out.push((
            rewritten.unwrap_or_else(|| condition.clone()),
            value.clone(),
        ));
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
    // Children were already normalized in their effective domains. Do
    // not recompute payloads with the default, domain-free context.
    for (cond, value) in branches {
        if let Some((prev_cond, prev_value)) = out.last_mut() {
            if prev_value == value {
                *prev_cond = crate::builder::or(prev_cond.clone(), cond.clone());
                changed = true;
                continue;
            }
        }
        out.push((cond.clone(), value.clone()));
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
        .map(|(_, value)| value.clone());

    // Grouping can only remove an arm when at least one payload repeats.
    // In particular a total CASE already emitted by this rule has unique
    // payloads, so do not rebuild its effective domains on later iterations.
    if explicit_default.is_some() {
        let mut seen_values = HashSet::new();
        if branches.iter().all(|(_, value)| seen_values.insert(value)) {
            return (branches.to_vec(), false);
        }
    }

    let effective_domains = first_match_effective_domains(branches);

    // Let the complete two-value partition rule own this case. The
    // grouping pass does not insert a default into an implicit partition.
    let raw_conds: Vec<Expr> = branches.iter().map(|(cond, _)| cond.clone()).collect();
    let raw_covered = crate::builder::conditions_cover_all(&raw_conds);
    let effective_covered = crate::builder::conditions_cover_all(&effective_domains);
    if raw_covered || effective_covered {
        let values: HashSet<_> = branches.iter().map(|(_, value)| value).collect();
        if values.len() == 2 {
            return (branches.to_vec(), false);
        }
    }

    struct ValueGroup {
        value: Expr,
        first: Expr,
        later: Vec<Expr>,
    }
    let mut groups: Vec<ValueGroup> = Vec::new();
    for ((_, value), domain) in branches.iter().zip(effective_domains) {
        let domain = crate::normalize(domain);
        if domain.as_bool() == Some(false) {
            continue;
        }
        if let Some(group) = groups.iter_mut().find(|group| &group.value == value) {
            group.later.push(domain);
        } else {
            groups.push(ValueGroup {
                value: value.clone(),
                first: domain,
                later: Vec::new(),
            });
        }
    }

    // Each group already has its first effective domain. Without a
    // repeated payload, grouping cannot eliminate an arm.
    if groups.iter().all(|group| group.later.is_empty()) {
        return (branches.to_vec(), false);
    }

    let mut out: Vec<(Expr, Expr)> = groups
        .into_iter()
        .map(|group| {
            let condition = group
                .later
                .into_iter()
                .fold(group.first, crate::builder::or);
            (crate::normalize(condition), group.value)
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

fn simplify_branch_conditions_under_prior_failures(
    branches: &[(Expr, Expr)],
) -> (Vec<(Expr, Expr)>, bool) {
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
        let mut conjuncts = flatten_and(cond);
        let original_len = conjuncts.len();
        // Reprove each deletion against what remains. Two terms can use
        // each other as evidence; neither proof alone licenses dropping both.
        for index in (0..original_len).rev() {
            let term = &conjuncts[index];
            if is_not_of_any(term, prev_conds)
                || is_not_conj_implied_by_prev_failure(term, &conjuncts, prev_conds)
            {
                conjuncts.remove(index);
            }
        }
        (conjuncts.len() != original_len).then(|| rebuild_and(&conjuncts))
    })
}

/// 检查 cond 是否是 NOT(expected) 对任一 prev_cond 成立。
fn is_not_of_any(cond: &Expr, prev_conds: &[&Expr]) -> bool {
    for prev in prev_conds {
        if is_not_of(cond, prev)
            || flatten_or(prev)
                .iter()
                .any(|disjunct| is_not_of(cond, disjunct))
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
                || super::integer_ranges::adjacent_acquired_integer_bounds(cond, expected_inner)
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
                    lhs.as_lit()
                        .and_then(crate::Lit::as_integer)
                        .map(|bound| (if op == Ge { Le } else { Ge }, rhs, bound))
                }
            }
            _ => None,
        }
    }
    let Some((op_a, subject_a, bound_a)) = closed_bound(a) else {
        return false;
    };
    let Some((op_b, subject_b, bound_b)) = closed_bound(b) else {
        return false;
    };
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
