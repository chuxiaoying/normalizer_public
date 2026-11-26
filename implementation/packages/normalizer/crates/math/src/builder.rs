//! Expr 工厂函数 —— 唯一的构造入口。
//!
//! 所有外部代码（包括 simulator 和 plugin）必须通过这里构造 Expr。
//! 模块外无法直接构造 `ExprKind`（字段 pub(crate)），杜绝裸构造。
//!
//! ## 使用示例
//!
//! ```text
//! use math::{builder::*, ScalarSymbolType};
//! let x = param("x", ScalarSymbolType::Integer);
//! let e = add(x, lit_int(1));  // x + 1
//! ```
//!
//! ## P0 架构断言（20260427）
//!
//! 普通标量构造器（param/sym_e/sym_e_path）只接收封闭的 ScalarSymbolType。
//! 完成、任务与复合时序绑定使用各自的构造器。
//! 业务 Pojo 必须用 HeapRef，禁止用 SymVar 表达。

use crate::domains::primitive::registry::FuncKey;
use crate::expr::{BinOpKind, Expr, ExprKind, Lit, UnaryOpKind};
use crate::symvar::{ScalarSymbolType, SymVarOrigin};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::AtomicU64;
use std::time::Instant;

static CONDITIONS_COVER_CALLS: AtomicU64 = AtomicU64::new(0);
static CONDITIONS_COVER_GROUP_SPLITS: AtomicU64 = AtomicU64::new(0);
static CONDITIONS_COVER_NORMALIZE_FALLBACKS: AtomicU64 = AtomicU64::new(0);
static CONDITIONS_COVER_DISJUNCTION_CACHE_MISSES: AtomicU64 = AtomicU64::new(0);
static CONDITIONS_COVER_DIRECT_COMPLEMENTS: AtomicU64 = AtomicU64::new(0);
static CONDITIONS_COVER_TRUTH_TABLE_PRECHECKS: AtomicU64 = AtomicU64::new(0);
static CONDITIONS_COVER_BUDGET_SKIPS: AtomicU64 = AtomicU64::new(0);
static CONDITIONS_COVER_CACHE_HITS: AtomicU64 = AtomicU64::new(0);
static CONDITIONS_COVER_CACHE_MISSES: AtomicU64 = AtomicU64::new(0);
static PATH_IMPLIES_CALLS: AtomicU64 = AtomicU64::new(0);
static PATH_IMPLIES_UNRELATED_SKIPS: AtomicU64 = AtomicU64::new(0);
static PATH_IMPLIES_BUDGET_SKIPS: AtomicU64 = AtomicU64::new(0);
static PATH_IMPLIES_NORMALIZE_FALLBACKS: AtomicU64 = AtomicU64::new(0);
static PATH_IMPLIES_INDEPENDENT_COUNTEREXAMPLES: AtomicU64 = AtomicU64::new(0);
static PATH_IMPLIES_CACHE_HITS: AtomicU64 = AtomicU64::new(0);
static PATH_IMPLIES_CACHE_MISSES: AtomicU64 = AtomicU64::new(0);


// Truth-table compilation is an exact proof, but compiling a boolean tree is
// linear in the complete tree for every condition and evaluation then repeats
// it for each assignment. A small number of atoms therefore does not imply a
// small proof problem: merged state branches can contain very large guards
// with only a handful of distinct atoms. Keep the proof conservative once the
// complete input exceeds this budget; callers retain the original branch
// rather than treating an unproved coverage claim as true.
const MAX_TRUTH_TABLE_INPUT_SIZE: usize = 32_768;

thread_local! {
    static CONDITIONS_COVER_CACHE: RefCell<HashMap<Vec<Expr>, bool>> = RefCell::new(HashMap::new());
    static CONDITIONS_COVER_DISJUNCTION_CACHE: RefCell<HashMap<Vec<Expr>, Expr>> = RefCell::new(HashMap::new());
    static PATH_IMPLIES_CACHE: RefCell<HashMap<(Expr, Expr), bool>> = RefCell::new(HashMap::new());
}

// =============================================================================
// 字面量
// =============================================================================

pub fn lit_int(n: i64) -> Expr {
    Expr::new(ExprKind::Lit(Lit::Int(n)))
}

/// Exact logical integer. i64 is only a compact storage optimization.
pub fn lit_integer(value: impl Into<num_bigint::BigInt>) -> Expr {
    use num_traits::ToPrimitive;
    let value = value.into();
    Expr::new(ExprKind::Lit(match value.to_i64() {
        Some(small) => Lit::Int(small),
        None => Lit::BigInt(value),
    }))
}

pub fn lit_str<S: Into<String>>(s: S) -> Expr {
    Expr::new(ExprKind::Lit(Lit::Str(s.into())))
}

pub fn lit_bool(b: bool) -> Expr {
    Expr::new(ExprKind::Lit(Lit::Bool(b)))
}

pub fn lit_null() -> Expr {
    Expr::new(ExprKind::Lit(Lit::Null))
}

/// 通用 Lit 构造器：包装任意 Lit 为 Expr（P2 方案 B：evaluator 返回值使用）
pub fn lit(l: Lit) -> Expr {
    match l {
        other => Expr::new(ExprKind::Lit(other)),
    }
}

// =============================================================================
// 符号变量（P0：所有 SymVar 构造强制 primitive_type）
// =============================================================================

/// Construct an entry parameter with an admitted logical scalar type.
pub fn param<N: Into<String>>(name: N, symbol_type: ScalarSymbolType) -> Expr {
    Expr::new(ExprKind::SymVar {
        origin: SymVarOrigin::Parameter {
            name: name.into(),
            primitive_type: symbol_type.tag().into(),
        },
    })
}

/// Construct an ordinary external scalar binding.
pub fn sym_e(binding_id: u64, symbol_type: ScalarSymbolType) -> Expr {
    sym_e_path(binding_id, Vec::new(), symbol_type)
}

/// A scalar projection of a declared external binding. The caller supplies
/// the binding and ordered field path directly, without intermediate Exprs.
pub fn sym_e_path(binding_id: u64, path: Vec<String>, symbol_type: ScalarSymbolType) -> Expr {
    Expr::new(ExprKind::SymVar {
        origin: SymVarOrigin::External {
            binding_id,
            path,
            primitive_type: symbol_type.tag().into(),
        },
    })
}

/// The immutable completion of one external request. Its payload type is
/// checked by the declared port before any value or exception projection.
pub fn external_completion(binding_id: u64) -> Expr {
    Expr::new(ExprKind::SymVar {
        origin: SymVarOrigin::External {
            binding_id,
            path: Vec::new(),
            primitive_type: "normalized.completion".into(),
        },
    })
}

/// Internal handle for one issued asynchronous task, never a PERS definition.
pub fn pending_task_handle(binding_id: u64) -> Expr {
    Expr::new(ExprKind::SymVar {
        origin: SymVarOrigin::Temporal {
            binding_id,
            primitive_type: "normalized.composite".into(),
        },
    })
}

pub const RECORD_TEMPORAL_TAG: &str = "normalized.choice_record";

pub fn record_temporal_result(binding_id: u64) -> Expr {
    Expr::new(ExprKind::SymVar {
        origin: SymVarOrigin::Temporal { binding_id, primitive_type: RECORD_TEMPORAL_TAG.into() },
    })
}

/// Exact integer produced by a closed, caller-owned temporal definition.
pub fn integer_temporal_result(binding_id: u64) -> Expr {
    Expr::new(ExprKind::SymVar {
        origin: SymVarOrigin::Temporal { binding_id, primitive_type: ScalarSymbolType::Integer.tag().into() },
    })
}

// =============================================================================
// HeapRef
// =============================================================================

/// 堆引用。id 由 executor 的 `Heap::new_object` 发放。
///
/// math 只持 u64，不负责解引用——对象字段读写由 executor 的 Heap 完成。
pub fn heap_ref(id: i64) -> Expr {
    Expr::new(ExprKind::HeapRef(id))
}

/// Relation row value/path atom.
///
/// This is deliberately not a SymVar: Pojo rows must not be faked as primitive
/// parameters.  The alias is still indexed as a relation-row variable so RCL
/// predicate splitting keeps it inside the relation.
pub fn row_value<T: Into<String>>(alias: T) -> Expr {
    Expr::new(ExprKind::RowValue { alias: alias.into(), acquired_integer: false })
}

/// A member acquired from a non-null exact-integer input array.
pub fn acquired_integer_row<T: Into<String>>(alias: T) -> Expr {
    Expr::new(ExprKind::RowValue { alias: alias.into(), acquired_integer: true })
}

// =============================================================================
// 比较与布尔算子
// =============================================================================

fn binop(op: BinOpKind, left: Expr, right: Expr) -> Expr {
    Expr::new(ExprKind::BinOp { op, left, right })
}

/// 通用 BinOp 构造器，供归约规则重建带明确算子的表达式。
///
/// 对外语义：任意 BinOpKind + 两个 Expr 构造 BinOp 表达式。设计上 builder 模块默认
/// 通过具名函数（eq/ne/lt/...）暴露 BinOp，但 normalize 规则需要动态 op 场景，
/// 故提供此 pub 入口。不泄漏 ExprKind（仍通过 Rc 封装）。
pub fn binop_public(op: BinOpKind, left: Expr, right: Expr) -> Expr {
    binop(op, left, right)
}

pub fn eq(left: Expr, right: Expr) -> Expr {
    binop(BinOpKind::Eq, left, right)
}
pub fn ne(left: Expr, right: Expr) -> Expr {
    binop(BinOpKind::Ne, left, right)
}
pub fn lt(left: Expr, right: Expr) -> Expr {
    binop(BinOpKind::Lt, left, right)
}
pub fn le(left: Expr, right: Expr) -> Expr {
    binop(BinOpKind::Le, left, right)
}
pub fn gt(left: Expr, right: Expr) -> Expr {
    binop(BinOpKind::Gt, left, right)
}
pub fn ge(left: Expr, right: Expr) -> Expr {
    binop(BinOpKind::Ge, left, right)
}

pub fn and(left: Expr, right: Expr) -> Expr {
    binop(BinOpKind::And, left, right)
}
pub fn or(left: Expr, right: Expr) -> Expr {
    binop(BinOpKind::Or, left, right)
}

pub fn not(operand: Expr) -> Expr {
    Expr::new(ExprKind::UnaryOp {
        op: UnaryOpKind::Not,
        operand,
    })
}

pub fn neg(operand: Expr) -> Expr {
    Expr::new(ExprKind::UnaryOp {
        op: UnaryOpKind::Neg,
        operand,
    })
}

// =============================================================================
// 算术
// =============================================================================

pub fn add(left: Expr, right: Expr) -> Expr {
    binop(BinOpKind::Add, left, right)
}
pub fn sub(left: Expr, right: Expr) -> Expr {
    binop(BinOpKind::Sub, left, right)
}
pub fn mul(left: Expr, right: Expr) -> Expr {
    binop(BinOpKind::Mul, left, right)
}
// =============================================================================
// CaseWhen
// =============================================================================

/// Construct a total first-match CASE. The fallback is unconditional.
pub fn case_when(mut arms: Vec<(Expr, Expr)>, fallback: Expr) -> Expr {
    arms.push((lit_bool(true), fallback));
    Expr::new(ExprKind::CaseWhen { branches: arms })
}

/// 构造 normal-return 域下的 partial CaseWhen。
///
/// 仅用于 executor/outcome 已经证明缺失分支由 `throws` 承载的场景：
/// R 只表达正常返回域，因此不应为了满足 total CaseWhen 形态而追加
/// `true -> null` 可见哨兵。普通值级 CaseWhen 仍必须使用 `case_when`，
/// 保持最后一支为 `true` 的 total 表达式 invariant。
pub fn normal_return_case_when(
    mut earlier_arms: Vec<(Expr, Expr)>,
    last_guarded_arm: (Expr, Expr),
) -> Expr {
    earlier_arms.push(last_guarded_arm);
    Expr::new(ExprKind::CaseWhen { branches: earlier_arms })
}

pub fn conditions_cover_all(conds: &[Expr]) -> bool {
    crate::profile::count(
        &CONDITIONS_COVER_CALLS,
        "math.proof.conditions_cover_all.call",
        |_| format!("conds={}", conds.len()),
    );
    let profile_start = crate::profile::enabled().then(Instant::now);
    if conds.iter().any(|cond| cond.as_bool() == Some(true)) {
        return true;
    }

    let relevant = canonicalize_coverage_conditions(
        conds
            .iter()
            .filter(|cond| cond.as_bool() != Some(false))
            .cloned()
            .collect(),
    );
    if relevant.is_empty() {
        return false;
    }

    let proof_input_size = relevant
        .iter()
        .map(Expr::size)
        .fold(0usize, usize::saturating_add);

    let decline = |cache_miss: bool| {
        let reason = if cache_miss { "run_proof_budget" } else { "input_size_budget" };
        crate::profile::count(
            &CONDITIONS_COVER_BUDGET_SKIPS,
            "math.proof.conditions_cover_all.budget_skip",
            |_| if cache_miss {
                format!("conds={} reason={}", relevant.len(), reason)
            } else {
                format!("conds={} size_budget={}", relevant.len(), MAX_TRUTH_TABLE_INPUT_SIZE)
            },
        );
        crate::profile::rule_event(
            "math.proof.conditions_cover_all",
            proof_input_size,
            0,
            profile_start.map_or(0, |start| start.elapsed().as_micros()),
            Some(reason),
        );
        false
    };

    // Decline before building a second copy when the input is too large.
    if proof_input_size > MAX_TRUTH_TABLE_INPUT_SIZE {
        return decline(false);
    }

    if let Some(cached) = conditions_cover_cache_get(&relevant) {
        crate::profile::count(
            &CONDITIONS_COVER_CACHE_HITS,
            "math.proof.conditions_cover_all.cache_hit",
            |_| format!("conds={}", relevant.len()),
        );
        crate::profile::rule_event(
            "math.proof.conditions_cover_all",
            proof_input_size,
            usize::from(cached),
            profile_start.map_or(0, |start| start.elapsed().as_micros()),
            None,
        );
        return cached;
    }
    crate::profile::count(
        &CONDITIONS_COVER_CACHE_MISSES,
        "math.proof.conditions_cover_all.cache_miss",
        |_| format!("conds={}", relevant.len()),
    );
    if !crate::normalize::consume_proof_budget(proof_input_size, proof_input_size) {
        return decline(true);
    }

    let publish_coverage = |reason: &str| {
        if let Some(start) = profile_start {
            crate::profile::event_elapsed_if_slow(
                "math.proof.conditions_cover_all",
                &format!("conds={} result={reason}", relevant.len()),
                start,
            );
        }
        conditions_cover_cache_insert(&relevant, true);
        crate::profile::rule_event(
            "math.proof.conditions_cover_all",
            proof_input_size,
            1,
            profile_start.map_or(0, |start| start.elapsed().as_micros()),
            None,
        );
        true
    };
    let result = if let Some(groups) = independent_condition_groups(&relevant) {
        crate::profile::count(
            &CONDITIONS_COVER_GROUP_SPLITS,
            "math.proof.conditions_cover_all.group_split",
            |_| format!("conds={} groups={}", relevant.len(), groups.len()),
        );
        groups
            .iter()
            .any(|group| conditions_cover_all(group.as_slice()))
    } else {
        crate::profile::count(
            &CONDITIONS_COVER_NORMALIZE_FALLBACKS,
            "math.proof.conditions_cover_all.normalize_fallback",
            |_| format!("conds={}", relevant.len()),
        );
        if conditions_have_direct_complement_pair(&relevant) {
            crate::profile::count(
                &CONDITIONS_COVER_DIRECT_COMPLEMENTS,
                "math.proof.conditions_cover_all.direct_complement",
                |_| format!("conds={}", relevant.len()),
            );
            return publish_coverage("true_by_direct_complement");
        }

        match boolean_truth_table_within_size_limit(&relevant) {
            TruthTableCoverage::CoversAll => {
                crate::profile::count(
                    &CONDITIONS_COVER_TRUTH_TABLE_PRECHECKS,
                    "math.proof.conditions_cover_all.truth_table_precheck",
                    |_| format!("conds={} result=true", relevant.len()),
                );
                return publish_coverage("true_by_truth_table");
            }
            TruthTableCoverage::DoesNotCoverAllExact => {
                crate::profile::count(
                    &CONDITIONS_COVER_TRUTH_TABLE_PRECHECKS,
                    "math.proof.conditions_cover_all.truth_table_precheck",
                    |_| format!("conds={} result=false_exact", relevant.len()),
                );
                false
            }
            TruthTableCoverage::Unknown => {
                let normalized_disjunction = normalized_disjunction_for_conditions(&relevant);
                if normalized_disjunction.as_bool() == Some(true) {
                    return publish_coverage("true_by_normalize");
                }

                // The precheck already exhausted the same input's truth
                // table (or declined it at the same budget). Normalization
                // above was the only additional proof; retain the branches.
                false
            }
        }
    };
    conditions_cover_cache_insert(&relevant, result);
    if let Some(start) = profile_start {
        crate::profile::event_elapsed_if_slow(
            "math.proof.conditions_cover_all",
            &format!("conds={} result={}", relevant.len(), result),
            start,
        );
    }
    crate::profile::rule_event(
        "math.proof.conditions_cover_all",
        proof_input_size,
        usize::from(result),
        profile_start.map_or(0, |start| start.elapsed().as_micros()),
        None,
    );
    result
}

fn conditions_cover_cache_get(conds: &[Expr]) -> Option<bool> {
    CONDITIONS_COVER_CACHE.with(|cache| cache.borrow().get(conds).copied())
}

fn conditions_cover_cache_insert(conds: &[Expr], value: bool) {
    let cache_weight: usize = conds.iter().map(Expr::size).sum();
    crate::cache::with_entry_budget(cache_weight, || {
        CONDITIONS_COVER_CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();
            crate::cache::insert_bounded(&mut cache, conds.to_vec(), value, crate::cache::DEFAULT_KEY_LIMIT, "conditions_cover");
        });
    });
}

fn canonicalize_coverage_conditions(mut conds: Vec<Expr>) -> Vec<Expr> {
    // Coverage is disjunction over conditions: order and duplicates do not
    // change semantics.  Canonicalizing here improves pure proof/disjunction
    // cache hit rate for adjacent callers that build the same coverage set in
    // slightly different orders, without introducing any scenario cache.
    conds.sort_by(|left, right| {
        left.structural_hash()
            .cmp(&right.structural_hash())
            .then_with(|| left.size().cmp(&right.size()))
            .then_with(|| left.depth().cmp(&right.depth()))
    });
    conds.dedup();
    conds
}

fn conditions_have_direct_complement_pair(conds: &[Expr]) -> bool {
    let mut positives: HashSet<Expr> = HashSet::new();
    let mut negatives: HashSet<Expr> = HashSet::new();

    for cond in conds {
        if let Some(operand) = cond.as_not() {
            if positives.contains(operand) {
                return true;
            }
            negatives.insert(operand.clone());
            continue;
        }

        if negatives.contains(cond) {
            return true;
        }
        positives.insert(cond.clone());
    }

    false
}

fn normalized_disjunction_for_conditions(conds: &[Expr]) -> Expr {

    crate::profile::count(
        &CONDITIONS_COVER_DISJUNCTION_CACHE_MISSES,
        "math.proof.conditions_cover_all.disjunction_cache_miss",
        |_| format!("conds={}", conds.len()),
    );

    let (cached_indexes, mut disjunction) =
        longest_cached_disjunction_subset(conds).unwrap_or_else(|| (vec![0], conds[0].clone()));

    for (idx, cond) in conds.iter().enumerate() {
        if cached_indexes.binary_search(&idx).is_ok() {
            continue;
        }
        disjunction = or(disjunction, cond.clone());
    }

    let normalized = crate::normalize(disjunction);
    conditions_cover_disjunction_cache_insert(conds, normalized.clone());
    normalized
}

fn longest_cached_disjunction_subset(conds: &[Expr]) -> Option<(Vec<usize>, Expr)> {
    if conds.len() < 2 {
        return None;
    }

    CONDITIONS_COVER_DISJUNCTION_CACHE.with(|cache| {
        let cache = cache.borrow();

        // Fast path: adjacent coverage calls often grow by appending one
        // condition. Keep this before the more general single-missing subset
        // probe.
        for prefix_len in (1..conds.len()).rev() {
            if let Some(expr) = cache.get(&conds[..prefix_len]) {
                return Some(((0..prefix_len).collect(), expr.clone()));
            }
        }

        // Canonical ordering means the new condition may be inserted anywhere,
        // not necessarily at the end. Probe N single-missing subsets so
        // conds=N+1 can reuse cached conds=N regardless of insertion position.
        // This is pure proof structure reuse: cached value is the normalized OR
        // of exactly that subset, and we still OR+normalize the missing cond.
        let mut candidate = Vec::with_capacity(conds.len() - 1);
        for missing_idx in 0..conds.len() {
            candidate.clear();
            for (idx, cond) in conds.iter().enumerate() {
                if idx != missing_idx {
                    candidate.push(cond.clone());
                }
            }
            if let Some(expr) = cache.get(&candidate) {
                let indexes = (0..conds.len()).filter(|idx| *idx != missing_idx).collect();
                return Some((indexes, expr.clone()));
            }
        }

        None
    })
}

fn conditions_cover_disjunction_cache_insert(conds: &[Expr], value: Expr) {
    let cache_weight = conds
        .iter()
        .map(Expr::size)
        .sum::<usize>()
        .saturating_add(value.size());
    crate::cache::with_entry_budget(cache_weight, || {
        CONDITIONS_COVER_DISJUNCTION_CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();
            crate::cache::insert_bounded(&mut cache, conds.to_vec(), value, crate::cache::DEFAULT_KEY_LIMIT, "conditions_cover_disjunction");
        });
    });
}

fn path_proof_cache_get(
    cache_cell: &'static std::thread::LocalKey<RefCell<HashMap<(Expr, Expr), bool>>>,
    active: &Expr,
    expr: &Expr,
) -> Option<bool> {
    cache_cell.with(|cache| cache.borrow().get(&(active.clone(), expr.clone())).copied())
}

fn path_proof_cache_insert(
    cache_cell: &'static std::thread::LocalKey<RefCell<HashMap<(Expr, Expr), bool>>>,
    active: &Expr,
    expr: &Expr,
    value: bool,
) {
    let cache_weight = active.size().saturating_add(expr.size());
    crate::cache::with_entry_budget(cache_weight, || {
        cache_cell.with(|cache| {
            let mut cache = cache.borrow_mut();
            crate::cache::insert_bounded(&mut cache, (active.clone(), expr.clone()), value, crate::cache::DEFAULT_KEY_LIMIT, "path_proof");
        });
    });
}

fn independent_condition_groups(conds: &[Expr]) -> Option<Vec<Vec<Expr>>> {
    if conds.len() < 2 {
        return None;
    }

    let mut groups: Vec<Vec<Expr>> = Vec::new();
    'next_cond: for cond in conds {
        for group in groups.iter_mut() {
            if group
                .iter()
                .any(|existing| proof_inputs_may_interact(existing, cond))
            {
                group.push(cond.clone());
                continue 'next_cond;
            }
        }
        groups.push(vec![cond.clone()]);
    }

    if groups.len() <= 1 {
        return None;
    }


    (groups.len() > 1).then_some(groups)
}

/// Fast profile gate for proof checks.
///
/// Heavy path proofs repeatedly build large boolean expressions and call the
/// normalizer.  If two expressions have no shared symbolic variables and cannot
/// interact through HeapRef identity, neither implication nor contradiction can
/// be proved by path-domain reasoning.  Returning "not proved" here is
/// conservative: tautologies may be missed, but no false proof is introduced.
fn proof_inputs_may_interact(left: &Expr, right: &Expr) -> bool {
    if !left.vars().is_disjoint(right.vars()) {
        return true;
    }
    left.has_heap_ref() && right.has_heap_ref()
}

fn proof_can_skip_as_unrelated(left: &Expr, right: &Expr) -> bool {
    // The caller has already ruled out a literal active guard.
    if right.as_bool().is_some() {
        return false;
    }
    !proof_inputs_may_interact(left, right)
}

/// Collect exact facts from conjunctions. A disjunction may be remembered
/// as one opaque fact, but never decomposed into unconditional facts.
fn exact_active_facts(active: &Expr, facts: &mut HashMap<Expr, bool>) -> bool {
    match active.kind() {
        ExprKind::Lit(Lit::Bool(true)) => true,
        ExprKind::BinOp { op: BinOpKind::And, left, right } =>
            exact_active_facts(left, facts) && exact_active_facts(right, facts),
        ExprKind::UnaryOp { op: UnaryOpKind::Not, operand } => {
            facts.insert(operand.clone(), false);
            facts.insert(not(operand.clone()), true);
            true
        }
        ExprKind::Lit(Lit::Bool(false))
        | ExprKind::CaseWhen { .. }
        | ExprKind::ArrayLit(_)
        | ExprKind::Relation(_)
        | ExprKind::RelationAggregate { .. } => false,
        _ => {
            facts.insert(active.clone(), true);
            facts.insert(not(active.clone()), false);
            true
        }
    }
}

fn evaluate_with_exact_facts(expression: &Expr, facts: &HashMap<Expr, bool>) -> Option<bool> {
    if let Some(value) = expression.as_bool() {
        return Some(value);
    }
    if let Some(value) = facts.get(expression) {
        return Some(*value);
    }
    if let Some(operand) = expression.as_not() {
        return evaluate_with_exact_facts(operand, facts).map(|value| !value);
    }
    let Some((op, left, right)) = expression.as_binop() else {
        return None;
    };
    match op {
        BinOpKind::And => {
            let left = evaluate_with_exact_facts(left, facts);
            if left == Some(false) {
                return Some(false);
            }
            let right = evaluate_with_exact_facts(right, facts);
            if right == Some(false) {
                return Some(false);
            }
            match (left, right) {
                (Some(true), Some(true)) => Some(true),
                _ => None,
            }
        }
        BinOpKind::Or => {
            let left = evaluate_with_exact_facts(left, facts);
            if left == Some(true) {
                return Some(true);
            }
            let right = evaluate_with_exact_facts(right, facts);
            match (left, right) {
                (Some(false), Some(false)) => Some(false),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Split an ordered CASE's remaining inputs between this arm and later arms.
pub fn first_match_domains(remaining: Expr, test: &Expr) -> (Expr, Expr) {
    if test.as_bool() == Some(true) {
        return (remaining, lit_bool(false));
    }
    (
        crate::normalize(and(remaining.clone(), test.clone())),
        crate::normalize(and(remaining, not(test.clone()))),
    )
}

/// Prove `active => expr` using the shared boolean normalizer/coverage engine.
pub fn path_implies(active: &Expr, expr: &Expr) -> bool {
    crate::profile::count(&PATH_IMPLIES_CALLS, "math.proof.path_implies.call", |_| {
        format!("active_size={} expr_size={}", active.size(), expr.size())
    });
    if expr.as_bool() == Some(true) || active.as_bool() == Some(false) || active == expr {
        return true;
    }
    if active.as_bool() == Some(true) {
        return expr.as_bool() == Some(true);
    }
    let profile_start = crate::profile::enabled().then(Instant::now);
    let proof_input_size = active.size().saturating_add(expr.size());
    if let Some(cached) = path_proof_cache_get(&PATH_IMPLIES_CACHE, active, expr) {
        crate::profile::count(
            &PATH_IMPLIES_CACHE_HITS,
            "math.proof.path_implies.cache_hit",
            |_| format!("active_size={} expr_size={}", active.size(), expr.size()),
        );
        crate::profile::rule_event(
            "math.proof.path_implies",
            proof_input_size,
            usize::from(cached),
            profile_start.map_or(0, |start| start.elapsed().as_micros()),
            None,
        );
        return cached;
    }
    let finish = |result: bool, skip_reason: Option<&str>| {
        // A depleted run cannot supply a durable proof result.
        if skip_reason != Some("run_proof_budget") {
            path_proof_cache_insert(&PATH_IMPLIES_CACHE, active, expr, result);
        }
        crate::profile::rule_event(
            "math.proof.path_implies",
            proof_input_size,
            usize::from(result),
            profile_start.map_or(0, |start| start.elapsed().as_micros()),
            skip_reason,
        );
        result
    };
    let mut exact_facts = HashMap::new();
    if exact_active_facts(active, &mut exact_facts) {
        if let Some(result) = evaluate_with_exact_facts(expr, &exact_facts) {
            return finish(result, Some("exact_active_facts"));
        }
    }
    crate::profile::count(
        &PATH_IMPLIES_CACHE_MISSES,
        "math.proof.path_implies.cache_miss",
        |_| format!("active_size={} expr_size={}", active.size(), expr.size()),
    );
    // The run budget limits cumulative proof work; the input threshold
    // additionally stops any one fallback from copying very large guards.
    let run_budget_skipped =
        !crate::normalize::consume_proof_budget(proof_input_size, proof_input_size);
    if run_budget_skipped || proof_input_size > MAX_TRUTH_TABLE_INPUT_SIZE {
        let reason = if run_budget_skipped { "run_proof_budget" } else { "input_size_budget" };
        crate::profile::count(
            &PATH_IMPLIES_BUDGET_SKIPS,
            "math.proof.path_implies.budget_skip",
            |_| {
                let context = if run_budget_skipped {
                    "reason=run_proof_budget".to_owned()
                } else {
                    format!("size_budget={MAX_TRUTH_TABLE_INPUT_SIZE}")
                };
                format!("active_size={} expr_size={} {context}", active.size(), expr.size())
            },
        );
        return finish(false, Some(reason));
    }
    // Only an exact negative table result replaces the downstream proof.
    // The synthetic NOT adds one node to the measured input.
    if proof_input_size < MAX_TRUTH_TABLE_INPUT_SIZE {
        let conditions = [not(active.clone()), expr.clone()];
        if boolean_truth_table_within_size_limit(&conditions)
            == TruthTableCoverage::DoesNotCoverAllExact
        {
            return finish(false, None);
        }
    }
    if proof_can_skip_as_unrelated(active, expr) {
        crate::profile::count(
            &PATH_IMPLIES_UNRELATED_SKIPS,
            "math.proof.path_implies.unrelated_skip",
            |_| format!("active_size={} expr_size={}", active.size(), expr.size()),
        );
        return finish(false, Some("unrelated_atoms"));
    }
    let counterexample_conditions = [active.clone(), not(expr.clone())];
    if boolean_conditions_have_independent_model(&counterexample_conditions) {
        crate::profile::count(
            &PATH_IMPLIES_INDEPENDENT_COUNTEREXAMPLES,
            "math.proof.path_implies.independent_counterexample",
            |_| format!("active_size={} expr_size={}", active.size(), expr.size()),
        );
        return finish(false, Some("independent_counterexample"));
    }
    crate::profile::count(
        &PATH_IMPLIES_NORMALIZE_FALLBACKS,
        "math.proof.path_implies.normalize_fallback",
        |_| format!("active_size={} expr_size={}", active.size(), expr.size()),
    );
    // Prove implication by refuting its counterexample. Keeping the active
    // conjunction intact lets the interval/contradiction solver use its facts
    // without distributing the negation of a large path into a disjunction.
    let result = crate::normalize(and(active.clone(), not(expr.clone()))).as_bool() == Some(false);
    finish(result, None)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TruthTableCoverage {
    CoversAll,
    DoesNotCoverAllExact,
    Unknown,
}

/// Callers have already checked the size of these conditions. The atom
/// limit and the distinction between exact and unknown results remain local.
fn boolean_truth_table_within_size_limit(conds: &[Expr]) -> TruthTableCoverage {
    let mut atoms: Vec<String> = Vec::new();
    for cond in conds {
        visit_boolean_atoms(cond, &mut |atom| atoms.push(boolean_atom_key(atom)));
    }
    atoms.sort();
    atoms.dedup();

    if atoms.len() > 16 {
        return TruthTableCoverage::Unknown;
    }

    let compiled = compile_boolean_truth_table_conditions(conds, &atoms);
    let covers_all = boolean_truth_table_covers_all_compiled(&compiled, atoms.len());
    if covers_all {
        return TruthTableCoverage::CoversAll;
    }

    if conds.iter().all(is_pure_boolean_propositional_expr) {
        return TruthTableCoverage::DoesNotCoverAllExact;
    }

    TruthTableCoverage::Unknown
}

#[derive(Clone, Debug)]
enum CompiledBoolExpr {
    Const(bool),
    Atom(usize),
    Not(Box<CompiledBoolExpr>),
    And(Box<CompiledBoolExpr>, Box<CompiledBoolExpr>),
    Or(Box<CompiledBoolExpr>, Box<CompiledBoolExpr>),
}

fn compile_boolean_truth_table_conditions(
    conds: &[Expr],
    atoms: &[String],
) -> Vec<CompiledBoolExpr> {
    let atom_index: HashMap<&str, usize> = atoms
        .iter()
        .enumerate()
        .map(|(idx, atom)| (atom.as_str(), idx))
        .collect();
    conds
        .iter()
        .map(|cond| compile_boolean_truth_table_expr(cond, &atom_index))
        .collect()
}

fn compile_boolean_truth_table_expr(
    expr: &Expr,
    atom_index: &HashMap<&str, usize>,
) -> CompiledBoolExpr {
    if let Some(value) = expr.as_bool() {
        return CompiledBoolExpr::Const(value);
    }
    if let Some(operand) = expr.as_not() {
        return CompiledBoolExpr::Not(Box::new(compile_boolean_truth_table_expr(
            operand, atom_index,
        )));
    }
    if let Some((op, left, right)) = expr.as_binop() {
        match op {
            BinOpKind::And => {
                return CompiledBoolExpr::And(
                    Box::new(compile_boolean_truth_table_expr(left, atom_index)),
                    Box::new(compile_boolean_truth_table_expr(right, atom_index)),
                );
            }
            BinOpKind::Or => {
                return CompiledBoolExpr::Or(
                    Box::new(compile_boolean_truth_table_expr(left, atom_index)),
                    Box::new(compile_boolean_truth_table_expr(right, atom_index)),
                );
            }
            BinOpKind::Eq => {
                let key = comparison_atom_key(left, right);
                let idx = *atom_index
                    .get(key.as_str())
                    .expect("comparison atom must have been collected");
                return CompiledBoolExpr::Atom(idx);
            }
            BinOpKind::Ne => {
                let key = comparison_atom_key(left, right);
                let idx = *atom_index
                    .get(key.as_str())
                    .expect("comparison atom must have been collected");
                return CompiledBoolExpr::Not(Box::new(CompiledBoolExpr::Atom(idx)));
            }
            _ => {}
        }
    }

    let key = boolean_atom_key(expr);
    let idx = *atom_index
        .get(key.as_str())
        .expect("boolean atom must have been collected");
    CompiledBoolExpr::Atom(idx)
}

fn boolean_truth_table_covers_all_compiled(conds: &[CompiledBoolExpr], atom_count: usize) -> bool {
    let total = 1u64 << atom_count;
    for mask in 0..total {
        let covered = conds
            .iter()
            .any(|cond| eval_compiled_boolean_expr(cond, mask));
        if !covered {
            return false;
        }
    }
    true
}

fn eval_compiled_boolean_expr(expr: &CompiledBoolExpr, mask: u64) -> bool {
    match expr {
        CompiledBoolExpr::Const(value) => *value,
        CompiledBoolExpr::Atom(idx) => ((mask >> idx) & 1) == 1,
        CompiledBoolExpr::Not(inner) => !eval_compiled_boolean_expr(inner, mask),
        CompiledBoolExpr::And(left, right) => {
            eval_compiled_boolean_expr(left, mask) && eval_compiled_boolean_expr(right, mask)
        }
        CompiledBoolExpr::Or(left, right) => {
            eval_compiled_boolean_expr(left, mask) || eval_compiled_boolean_expr(right, mask)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PartialBooleanValue {
    True,
    False,
    Unknown,
}

fn eval_compiled_boolean_partial(
    expr: &CompiledBoolExpr,
    assignments: &[Option<bool>],
) -> PartialBooleanValue {
    match expr {
        CompiledBoolExpr::Const(value) => {
            if *value {
                PartialBooleanValue::True
            } else {
                PartialBooleanValue::False
            }
        }
        CompiledBoolExpr::Atom(index) => match assignments[*index] {
            Some(true) => PartialBooleanValue::True,
            Some(false) => PartialBooleanValue::False,
            None => PartialBooleanValue::Unknown,
        },
        CompiledBoolExpr::Not(inner) => match eval_compiled_boolean_partial(inner, assignments) {
            PartialBooleanValue::True => PartialBooleanValue::False,
            PartialBooleanValue::False => PartialBooleanValue::True,
            PartialBooleanValue::Unknown => PartialBooleanValue::Unknown,
        },
        CompiledBoolExpr::And(left, right) => {
            let left = eval_compiled_boolean_partial(left, assignments);
            let right = eval_compiled_boolean_partial(right, assignments);
            if left == PartialBooleanValue::False || right == PartialBooleanValue::False {
                PartialBooleanValue::False
            } else if left == PartialBooleanValue::True && right == PartialBooleanValue::True {
                PartialBooleanValue::True
            } else {
                PartialBooleanValue::Unknown
            }
        }
        CompiledBoolExpr::Or(left, right) => {
            let left = eval_compiled_boolean_partial(left, assignments);
            let right = eval_compiled_boolean_partial(right, assignments);
            if left == PartialBooleanValue::True || right == PartialBooleanValue::True {
                PartialBooleanValue::True
            } else if left == PartialBooleanValue::False && right == PartialBooleanValue::False {
                PartialBooleanValue::False
            } else {
                PartialBooleanValue::Unknown
            }
        }
    }
}

fn find_boolean_model(
    conditions: &[CompiledBoolExpr],
    assignments: &mut [Option<bool>],
    next_atom: usize,
    budget: &mut usize,
) -> bool {
    if *budget == 0 {
        return false;
    }
    *budget -= 1;

    if conditions.iter().any(|condition| {
        eval_compiled_boolean_partial(condition, assignments) == PartialBooleanValue::False
    }) {
        return false;
    }
    if conditions.iter().all(|condition| {
        eval_compiled_boolean_partial(condition, assignments) == PartialBooleanValue::True
    }) {
        return true;
    }
    // With all atoms assigned, the tests above have resolved every condition.
    assignments[next_atom] = Some(true);
    if find_boolean_model(conditions, assignments, next_atom + 1, budget) {
        return true;
    }
    assignments[next_atom] = Some(false);
    if find_boolean_model(conditions, assignments, next_atom + 1, budget) {
        return true;
    }
    assignments[next_atom] = None;
    false
}

fn visit_boolean_atoms(expr: &Expr, visit: &mut impl FnMut(&Expr)) {
    if expr.as_bool().is_some() {
        return;
    }
    if let Some(operand) = expr.as_not() {
        visit_boolean_atoms(operand, visit);
    } else if let Some((BinOpKind::And | BinOpKind::Or, left, right)) = expr.as_binop() {
        visit_boolean_atoms(left, visit);
        visit_boolean_atoms(right, visit);
    } else {
        visit(expr);
    }
}

fn boolean_atom_key(expr: &Expr) -> String {
    if let Some((BinOpKind::Eq | BinOpKind::Ne, left, right)) = expr.as_binop() {
        comparison_atom_key(left, right)
    } else {
        format!("{:?}", expr)
    }
}

/// Find a concrete propositional counterexample for a conjunction of formulas
/// when every distinct atom is independent of every other atom. This is a
/// deliberately conservative negative proof. It never proves implication and
/// it declines formulas whose atoms have no symbolic carrier, share symbols,
/// or share HeapRef identity, where call, arithmetic, relation, or heap
/// semantics may correlate truth values.
fn boolean_conditions_have_independent_model(conditions: &[Expr; 2]) -> bool {
    let mut atom_exprs = Vec::new();
    for condition in conditions {
        visit_boolean_atoms(condition, &mut |atom| atom_exprs.push(atom.clone()));
    }
    let mut atoms = Vec::new();
    let mut atom_values = HashMap::<String, Expr>::new();
    for atom in atom_exprs {
        let key = boolean_atom_key(&atom);
        if !atom_values.contains_key(&key) {
            atom_values.insert(key, atom.clone());
            atoms.push(atom);
        }
    }
    for (index, atom) in atoms.iter().enumerate() {
        if atom.vars().is_empty() || atom.has_case_when() {
            return false;
        }
        for other in atoms.iter().skip(index + 1) {
            if proof_inputs_may_interact(atom, other) {
                return false;
            }
        }
    }

    let mut atom_keys: Vec<String> = atom_values.keys().cloned().collect();
    atom_keys.sort();
    let compiled = compile_boolean_truth_table_conditions(conditions, &atom_keys);
    let mut assignments = vec![None; atom_keys.len()];
    find_boolean_model(&compiled, &mut assignments, 0, &mut 20_000)
}

fn is_pure_boolean_propositional_expr(expr: &Expr) -> bool {
    if expr.as_bool().is_some() {
        return true;
    }
    if let Some(operand) = expr.as_not() {
        return is_pure_boolean_propositional_expr(operand);
    }
    if let Some((op, left, right)) = expr.as_binop() {
        return matches!(op, BinOpKind::And | BinOpKind::Or)
            && is_pure_boolean_propositional_expr(left)
            && is_pure_boolean_propositional_expr(right);
    }
    expr.as_sym_var().is_some_and(|origin| origin.primitive_type() == "boolean")
}

fn comparison_atom_key(left: &Expr, right: &Expr) -> String {
    let a = format!("{:?}", left);
    let b = format!("{:?}", right);
    if a <= b {
        format!("{} == {}", a, b)
    } else {
        format!("{} == {}", b, a)
    }
}

// =============================================================================
// Call（函数调用）
// =============================================================================

/// 构造已注册函数的调用。
///
pub fn call(key: FuncKey, args: Vec<Expr>) -> Expr {
    Expr::new(ExprKind::Call {
        key,
        args,
    })
}

// =============================================================================
// ArrayLit
// =============================================================================

/// 构造数组字面量。
pub fn array_lit(elements: Vec<Expr>) -> Expr {
    Expr::new(ExprKind::ArrayLit(elements))
}

#[cfg(test)]
mod coverage_proof_tests {
    use super::*;

    #[test]
    fn jointly_exhaustive_integer_guards_need_integer_reasoning() {
        let x = param("x", ScalarSymbolType::Integer);
        let y = param("y", ScalarSymbolType::Integer);
        let zero = lit_int(0);
        let below = lt(x.clone(), zero.clone());
        let at = eq(x.clone(), zero.clone());
        let above = gt(x, zero.clone());

        assert!(matches!(
            boolean_truth_table_within_size_limit(&[below.clone(), at.clone(), above.clone()]),
            TruthTableCoverage::Unknown
        ));
        assert!(conditions_cover_all(&[below.clone(), at.clone(), above]));
        // x=1, y=0 is a counterexample once the upper guard changes subject.
        assert!(!conditions_cover_all(&[below, at, gt(y, zero)]));
    }

    #[test]
    fn direct_complement_pair_is_recognized_in_both_orders() {
        let p = param("p", ScalarSymbolType::Boolean);
        let q = param("q", ScalarSymbolType::Boolean);

        assert!(conditions_have_direct_complement_pair(&[p.clone(), not(p.clone())]));
        assert!(conditions_have_direct_complement_pair(&[not(p.clone()), p.clone()]));
        assert!(!conditions_have_direct_complement_pair(&[p, not(q)]));
    }
}
