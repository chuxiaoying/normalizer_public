//! Expr 工厂函数 —— 唯一的构造入口。
//!
//! 所有外部代码（包括 simulator 和 plugin）必须通过这里构造 Expr。
//! 模块外无法直接构造 `ExprKind`（字段 pub(crate)），杜绝裸构造。
//!
//! ## 使用示例
//!
//! ```text
//! use math::builder::*;
//! let x = param("x", "integer");
//! let e = add(x, lit_int(1));  // x + 1
//! ```
//!
//! ## P0 架构断言（20260427）
//!
//! 所有 SymVar 构造（param/sym_e/sym_e_project/sym_t）必须传入 primitive_type，
//! 普通标量构造器校验语言适配器交付的逻辑类型，发布版本也同样校验。
//! 业务 Pojo 必须用 HeapRef，禁止用 SymVar 表达。

use crate::domains::primitive::registry::{func_arity, FuncKey};
use crate::expr::{BinOpKind, Expr, ExprKind, Lit, UnaryOpKind};
use crate::symvar::SymVarOrigin;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::AtomicU64;
use std::time::Instant;

static CONDITIONS_COVER_CALLS: AtomicU64 = AtomicU64::new(0);
static CONDITIONS_COVER_GROUP_SPLITS: AtomicU64 = AtomicU64::new(0);
static CONDITIONS_COVER_NORMALIZE_FALLBACKS: AtomicU64 = AtomicU64::new(0);
static CONDITIONS_COVER_DISJUNCTION_CACHE_HITS: AtomicU64 = AtomicU64::new(0);
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

fn emit_proof_rule_event(
    category: &str,
    input_size: usize,
    output_size: usize,
    start: Option<Instant>,
    cache_hit: bool,
    cache_miss: bool,
    budget_skip: bool,
    result: bool,
    skip_reason: Option<&str>,
    extra: &[(&str, String)],
) {
    let elapsed_us = start.map_or(0, |started| started.elapsed().as_micros());
    let mut fields = vec![
        ("proof_input_size", input_size.to_string()),
        ("cache_hit", usize::from(cache_hit).to_string()),
        ("cache_miss", usize::from(cache_miss).to_string()),
        ("budget_skip", usize::from(budget_skip).to_string()),
        ("proof_result", result.to_string()),
    ];
    fields.extend(extra.iter().map(|(key, value)| (*key, value.clone())));
    crate::profile::rule_event(
        category,
        input_size,
        output_size,
        elapsed_us,
        skip_reason,
        &fields,
    );
}

fn emit_path_implies_event(
    active: &Expr,
    expr: &Expr,
    output_size: usize,
    start: Option<Instant>,
    cache_hit: bool,
    cache_miss: bool,
    budget_skip: bool,
    result: bool,
    skip_reason: Option<&str>,
) {
    let extra = [
        ("active_hash", active.structural_hash().to_string()),
        ("expr_hash", expr.structural_hash().to_string()),
        ("active_size", active.size().to_string()),
        ("expr_size", expr.size().to_string()),
        ("active_depth", active.depth().to_string()),
        ("expr_depth", expr.depth().to_string()),
        ("active_case_when", active.has_case_when().to_string()),
        ("expr_case_when", expr.has_case_when().to_string()),
    ];
    emit_proof_rule_event(
        "math.proof.path_implies",
        active.size().saturating_add(expr.size()),
        output_size,
        start,
        cache_hit,
        cache_miss,
        budget_skip,
        result,
        skip_reason,
        &extra,
    );
}

thread_local! {
    static CONDITIONS_COVER_CACHE: RefCell<HashMap<Vec<Expr>, bool>> = RefCell::new(HashMap::new());
    static CONDITIONS_COVER_DISJUNCTION_CACHE: RefCell<HashMap<Vec<Expr>, Expr>> = RefCell::new(HashMap::new());
    static PATH_IMPLIES_CACHE: RefCell<HashMap<(Expr, Expr), bool>> = RefCell::new(HashMap::new());
}

// =============================================================================
// P0 内部断言 helper
// =============================================================================

/// 断言 primitive_type 是合法 primitive。
///
/// Both debug and release builds validate the type.
/// 用户指示：SymVar 必须 primitive，业务 Pojo 用 HeapRef。
#[inline]
fn assert_primitive_type(ty: &str) {
    assert!(
        crate::Primitive::from_symbol_tag(ty).is_some(),
        "[MATH-BUILDER] SymVar primitive_type must be primitive (closed scalar encoding), got: '{}'. Pojo types must use HeapRef, not SymVar.",
        ty
    );
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
        Lit::BigInt(value) => lit_integer(value),
        other => Expr::new(ExprKind::Lit(other)),
    }
}

// =============================================================================
// 符号变量（P0：所有 SymVar 构造强制 primitive_type）
// =============================================================================

/// 入参符号（P0：必填 primitive_type）。
///
/// # 示例
/// ```text
/// use math::builder::param;
/// let x = param("x", "integer");
/// let s = param("s", "text");
/// ```
///
/// # Panics
/// 如果 primitive_type 不是 primitive（如传入 `"biz.Pojo"`），assert panic（release mode 也生效）。
/// Pojo 值必须用 HeapRef 表达。
pub fn param<N: Into<String>, T: Into<String>>(name: N, primitive_type: T) -> Expr {
    let ty = primitive_type.into();
    assert_primitive_type(&ty);
    Expr::new(ExprKind::SymVar {
        origin: SymVarOrigin::Parameter {
            name: name.into(),
            primitive_type: ty,
        },
    })
}

/// 外部寄存符号（E）。binding_id 由外部发放（P0：必填 primitive_type）。
///
/// # Panics
/// primitive_type 必须是 primitive。Boundary 返回 Pojo 场景应改走 HeapRef
/// （方向1独立 PR 处理）。
pub fn sym_e<T: Into<String>>(binding_id: u64, primitive_type: T) -> Expr {
    let ty = primitive_type.into();
    assert_primitive_type(&ty);
    Expr::new(ExprKind::SymVar {
        origin: SymVarOrigin::External {
            binding_id,
            path: Vec::new(),
            primitive_type: ty,
        },
    })
}

/// 对 E 符号做字段投影，产生新的 E 符号（P0：必填投影字段的 primitive_type）。
///
/// # Panics
/// - base 不是 External SymVar → panic
/// - primitive_type 不是 primitive（assert panic，release mode 也生效）
pub fn sym_e_project<T: Into<String>>(base: &Expr, field: &str, primitive_type: T) -> Expr {
    let ty = primitive_type.into();
    assert_primitive_type(&ty);
    match base.kind() {
        ExprKind::SymVar {
            origin: SymVarOrigin::External {
                binding_id, path, ..
            },
        } => {
            let mut new_path = path.clone();
            new_path.push(field.to_string());
            Expr::new(ExprKind::SymVar {
                origin: SymVarOrigin::External {
                    binding_id: *binding_id,
                    path: new_path,
                    primitive_type: ty,
                },
            })
        }
        _ => panic!(
            "[MATH-BUILDER] sym_e_project: base must be an External SymVar, got {:?}",
            base
        ),
    }
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
        origin: SymVarOrigin::Temporal { binding_id, primitive_type: "integer".into() },
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

/// 构造 CaseWhen 分支。至少一个分支，最后一个分支条件必须是 `lit_bool(true)` 兜底。
///
/// # Panics
/// - branches 为空
/// - 最后一个分支条件不是 `true`
pub fn case_when(branches: Vec<(Expr, Expr)>) -> Expr {
    assert!(
        !branches.is_empty(),
        "[MATH-BUILDER] case_when: branches must not be empty"
    );
    if let Some((last_cond, _)) = branches.last() {
        assert!(
            last_cond.as_bool() == Some(true),
            "[MATH-BUILDER] case_when: last branch condition must be lit_bool(true), got {:?}",
            last_cond
        );
    }
    Expr::new(ExprKind::CaseWhen { branches })
}

/// 构造 normal-return 域下的 partial CaseWhen。
///
/// 仅用于 executor/outcome 已经证明缺失分支由 `throws` 承载的场景：
/// R 只表达正常返回域，因此不应为了满足 total CaseWhen 形态而追加
/// `true -> null` 可见哨兵。普通值级 CaseWhen 仍必须使用 `case_when`，
/// 保持最后一支为 `true` 的 total 表达式 invariant。
pub fn normal_return_case_when(branches: Vec<(Expr, Expr)>) -> Expr {
    assert!(
        !branches.is_empty(),
        "[MATH-BUILDER] normal_return_case_when: branches must not be empty"
    );
    Expr::new(ExprKind::CaseWhen { branches })
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

    // Coverage is an optimization proof. If the merged guards are too large,
    // decline the proof before constructing a second copy for truth-table or
    // disjunction normalization; retaining the branch is the sound fallback.
    if !truth_table_input_within_budget(&relevant) {
        crate::profile::count(
            &CONDITIONS_COVER_BUDGET_SKIPS,
            "math.proof.conditions_cover_all.budget_skip",
            |_| {
                format!(
                    "conds={} size_budget={}",
                    relevant.len(),
                    MAX_TRUTH_TABLE_INPUT_SIZE
                )
            },
        );
        emit_proof_rule_event(
            "math.proof.conditions_cover_all",
            proof_input_size,
            0,
            profile_start,
            false,
            false,
            true,
            false,
            Some("input_size_budget"),
            &[("condition_count", relevant.len().to_string())],
        );
        return false;
    }

    if let Some(cached) = conditions_cover_cache_get(&relevant) {
        crate::profile::count(
            &CONDITIONS_COVER_CACHE_HITS,
            "math.proof.conditions_cover_all.cache_hit",
            |_| format!("conds={}", relevant.len()),
        );
        emit_proof_rule_event(
            "math.proof.conditions_cover_all",
            proof_input_size,
            usize::from(cached),
            profile_start,
            true,
            false,
            false,
            cached,
            None,
            &[("condition_count", relevant.len().to_string())],
        );
        return cached;
    }
    crate::profile::count(
        &CONDITIONS_COVER_CACHE_MISSES,
        "math.proof.conditions_cover_all.cache_miss",
        |_| format!("conds={}", relevant.len()),
    );
    if !crate::normalize::consume_proof_budget(proof_input_size, proof_input_size) {
        crate::profile::count(
            &CONDITIONS_COVER_BUDGET_SKIPS,
            "math.proof.conditions_cover_all.budget_skip",
            |_| format!("conds={} reason=run_proof_budget", relevant.len()),
        );
        emit_proof_rule_event(
            "math.proof.conditions_cover_all",
            proof_input_size,
            0,
            profile_start,
            false,
            true,
            true,
            false,
            Some("run_proof_budget"),
            &[("condition_count", relevant.len().to_string())],
        );
        return false;
    }

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
            if let Some(start) = profile_start {
                crate::profile::event_elapsed_if_slow(
                    "math.proof.conditions_cover_all",
                    &format!("conds={} result=true_by_direct_complement", relevant.len()),
                    start,
                );
            }
            conditions_cover_cache_insert(&relevant, true);
            emit_proof_rule_event(
                "math.proof.conditions_cover_all",
                proof_input_size,
                1,
                profile_start,
                false,
                true,
                false,
                true,
                None,
                &[("condition_count", relevant.len().to_string())],
            );
            return true;
        }

        match boolean_truth_table_covers_all_precheck(&relevant) {
            TruthTableCoverage::CoversAll => {
                crate::profile::count(
                    &CONDITIONS_COVER_TRUTH_TABLE_PRECHECKS,
                    "math.proof.conditions_cover_all.truth_table_precheck",
                    |_| format!("conds={} result=true", relevant.len()),
                );
                if let Some(start) = profile_start {
                    crate::profile::event_elapsed_if_slow(
                        "math.proof.conditions_cover_all",
                        &format!("conds={} result=true_by_truth_table", relevant.len()),
                        start,
                    );
                }
                conditions_cover_cache_insert(&relevant, true);
                emit_proof_rule_event(
                    "math.proof.conditions_cover_all",
                    proof_input_size,
                    1,
                    profile_start,
                    false,
                    true,
                    false,
                    true,
                    None,
                    &[("condition_count", relevant.len().to_string())],
                );
                return true;
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
                    if let Some(start) = profile_start {
                        crate::profile::event_elapsed_if_slow(
                            "math.proof.conditions_cover_all",
                            &format!("conds={} result=true_by_normalize", relevant.len()),
                            start,
                        );
                    }
                    conditions_cover_cache_insert(&relevant, true);
                    emit_proof_rule_event(
                        "math.proof.conditions_cover_all",
                        proof_input_size,
                        1,
                        profile_start,
                        false,
                        true,
                        false,
                        true,
                        None,
                        &[("condition_count", relevant.len().to_string())],
                    );
                    return true;
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
    emit_proof_rule_event(
        "math.proof.conditions_cover_all",
        proof_input_size,
        usize::from(result),
        profile_start,
        false,
        true,
        false,
        result,
        None,
        &[("condition_count", relevant.len().to_string())],
    );
    result
}

fn conditions_cover_cache_get(conds: &[Expr]) -> Option<bool> {
    let cached = CONDITIONS_COVER_CACHE.with(|cache| cache.borrow().get(conds).copied());
    if crate::profile::cache_observation_enabled() {
        crate::profile::cache_lookup(
            "proof",
            "conditions_cover",
            cached.is_some(),
            proof_key_hash(conds),
            conds.iter().map(Expr::size).sum(),
            "conditions_cover_all",
        );
    }
    cached
}

fn conditions_cover_cache_insert(conds: &[Expr], value: bool) {
    let cache_weight: usize = conds.iter().map(Expr::size).sum();
    if cache_weight > crate::profile::cache_entry_limit() {
        crate::profile::cache_admission_skip("conditions_cover", cache_weight);
        return;
    }
    CONDITIONS_COVER_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        crate::cache::insert_bounded(&mut cache, conds.to_vec(), value, crate::cache::DEFAULT_KEY_LIMIT, "conditions_cover");
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
        if let Some((op, operand)) = cond.as_unary_op() {
            if op == UnaryOpKind::Not {
                if positives.contains(operand) {
                    return true;
                }
                negatives.insert(operand.clone());
                continue;
            }
        }

        if negatives.contains(cond) {
            return true;
        }
        positives.insert(cond.clone());
    }

    false
}

fn normalized_disjunction_for_conditions(conds: &[Expr]) -> Expr {
    if let Some(cached) = conditions_cover_disjunction_cache_get(conds) {
        crate::profile::count(
            &CONDITIONS_COVER_DISJUNCTION_CACHE_HITS,
            "math.proof.conditions_cover_all.disjunction_cache_hit",
            |_| format!("conds={}", conds.len()),
        );
        return cached;
    }

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

fn conditions_cover_disjunction_cache_get(conds: &[Expr]) -> Option<Expr> {
    let cached =
        CONDITIONS_COVER_DISJUNCTION_CACHE.with(|cache| cache.borrow().get(conds).cloned());
    if crate::profile::cache_observation_enabled() {
        crate::profile::cache_lookup(
            "proof",
            "conditions_cover_disjunction",
            cached.is_some(),
            proof_key_hash(conds),
            conds.iter().map(Expr::size).sum(),
            "conditions_cover_all",
        );
    }
    cached
}

fn conditions_cover_disjunction_cache_insert(conds: &[Expr], value: Expr) {
    let cache_weight = conds
        .iter()
        .map(Expr::size)
        .sum::<usize>()
        .saturating_add(value.size());
    if cache_weight > crate::profile::cache_entry_limit() {
        crate::profile::cache_admission_skip("conditions_cover_disjunction", cache_weight);
        return;
    }
    CONDITIONS_COVER_DISJUNCTION_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        crate::cache::insert_bounded(&mut cache, conds.to_vec(), value, crate::cache::DEFAULT_KEY_LIMIT, "conditions_cover_disjunction");
    });
}

fn path_proof_cache_get(
    cache_cell: &'static std::thread::LocalKey<RefCell<HashMap<(Expr, Expr), bool>>>,
    active: &Expr,
    expr: &Expr,
    cache_kind: &str,
) -> Option<bool> {
    let cached =
        cache_cell.with(|cache| cache.borrow().get(&(active.clone(), expr.clone())).copied());
    if crate::profile::cache_observation_enabled() {
        crate::profile::cache_lookup(
            "proof",
            cache_kind,
            cached.is_some(),
            proof_pair_hash(active, expr),
            active.size().saturating_add(expr.size()),
            cache_kind,
        );
    }
    cached
}

fn proof_key_hash(exprs: &[Expr]) -> u64 {
    exprs.iter().fold(0x9e37_79b9_7f4a_7c15, |hash, expr| {
        hash.rotate_left(7) ^ expr.structural_hash()
    })
}

fn proof_pair_hash(active: &Expr, expr: &Expr) -> u64 {
    active.structural_hash().rotate_left(17) ^ expr.structural_hash()
}

fn path_proof_cache_insert(
    cache_cell: &'static std::thread::LocalKey<RefCell<HashMap<(Expr, Expr), bool>>>,
    active: &Expr,
    expr: &Expr,
    value: bool,
) {
    let cache_weight = active.size().saturating_add(expr.size());
    if cache_weight > crate::profile::cache_entry_limit() {
        crate::profile::cache_admission_skip("path_proof", cache_weight);
        return;
    }
    cache_cell.with(|cache| {
        let mut cache = cache.borrow_mut();
        crate::cache::insert_bounded(&mut cache, (active.clone(), expr.clone()), value, crate::cache::DEFAULT_KEY_LIMIT, "path_proof");
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

    // Merge transitive interactions: A~B and B~C means all three belong to the
    // same proof component even if A and C do not directly share symbols.
    let mut changed = true;
    while changed {
        changed = false;
        'outer: for i in 0..groups.len() {
            for j in (i + 1)..groups.len() {
                let interacts = groups[i].iter().any(|left| {
                    groups[j]
                        .iter()
                        .any(|right| proof_inputs_may_interact(left, right))
                });
                if interacts {
                    let mut merged = groups.remove(j);
                    groups[i].append(&mut merged);
                    changed = true;
                    break 'outer;
                }
            }
        }
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
    if left == right {
        return true;
    }
    if !left.vars().is_disjoint(right.vars()) {
        return true;
    }
    left.has_heap_ref() && right.has_heap_ref()
}

fn proof_can_skip_as_unrelated(left: &Expr, right: &Expr) -> bool {
    if left.as_bool().is_some() || right.as_bool().is_some() {
        return false;
    }
    !proof_inputs_may_interact(left, right)
}

// Positive Boolean proofs only. Exhausting this local search proves nothing;
// the complete arithmetic/Boolean proof below remains responsible for the query.
fn structural_implication(active: &Expr, goal: &Expr, budget: &mut usize) -> bool {
    if *budget == 0 {
        return false;
    }
    *budget -= 1;
    if active == goal || goal.as_bool() == Some(true) || active.as_bool() == Some(false) {
        return true;
    }
    if let Some((BinOpKind::And, l, r)) = goal.as_binop() {
        if structural_implication(active, l, budget) && structural_implication(active, r, budget) {
            return true;
        }
    }
    if let Some((BinOpKind::Or, l, r)) = active.as_binop() {
        if structural_implication(l, goal, budget) && structural_implication(r, goal, budget) {
            return true;
        }
    }
    if let Some((BinOpKind::Or, l, r)) = goal.as_binop() {
        if structural_implication(active, l, budget) || structural_implication(active, r, budget) {
            return true;
        }
    }
    if let Some((BinOpKind::And, l, r)) = active.as_binop() {
        return structural_implication(l, goal, budget) || structural_implication(r, goal, budget);
    }
    false
}

/// Collect exact Boolean facts from a conjunction without interpreting any
/// arithmetic, call, heap, or relation semantics. A disjunction in the active
/// path is deliberately rejected because it cannot be represented as one
/// literal fact set.
fn exact_active_facts(active: &Expr, facts: &mut HashMap<Expr, bool>) -> bool {
    if active.as_bool() == Some(true) {
        return true;
    }
    if let Some((BinOpKind::And, left, right)) = active.as_binop() {
        return exact_active_facts(left, facts) && exact_active_facts(right, facts);
    }
    if let Some((UnaryOpKind::Not, operand)) = active.as_unary_op() {
        if operand.as_bool().is_some()
            || operand
                .as_binop()
                .is_some_and(|(op, _, _)| matches!(op, BinOpKind::And | BinOpKind::Or))
        {
            return false;
        }
        facts.insert(operand.clone(), false);
        facts.insert(not(operand.clone()), true);
        return true;
    }
    if active.as_bool().is_some()
        || active.as_case_when().is_some()
        || active.as_array_lit().is_some()
        || active.as_relation().is_some()
        || active.as_relation_aggregate().is_some()
    {
        return false;
    }
    facts.insert(active.clone(), true);
    facts.insert(not(active.clone()), false);
    true
}

fn evaluate_with_exact_facts(expression: &Expr, facts: &HashMap<Expr, bool>) -> Option<bool> {
    if let Some(value) = expression.as_bool() {
        return Some(value);
    }
    if let Some(value) = facts.get(expression) {
        return Some(*value);
    }
    if let Some((UnaryOpKind::Not, operand)) = expression.as_unary_op() {
        return evaluate_with_exact_facts(operand, facts).map(|value| !value);
    }
    if let Some(branches) = expression.as_case_when() {
        for (condition, value) in branches {
            match evaluate_with_exact_facts(condition, facts) {
                Some(true) => return evaluate_with_exact_facts(value, facts),
                Some(false) => continue,
                None => return None,
            }
        }
        return None;
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
            if right == Some(true) {
                return Some(true);
            }
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
    if let Some(cached) = path_proof_cache_get(&PATH_IMPLIES_CACHE, active, expr, "path_implies") {
        crate::profile::count(
            &PATH_IMPLIES_CACHE_HITS,
            "math.proof.path_implies.cache_hit",
            |_| format!("active_size={} expr_size={}", active.size(), expr.size()),
        );
        emit_path_implies_event(
            active,
            expr,
            usize::from(cached),
            profile_start,
            true,
            false,
            false,
            cached,
            None,
        );
        return cached;
    }
    let mut exact_facts = HashMap::new();
    if exact_active_facts(active, &mut exact_facts) {
        if let Some(result) = evaluate_with_exact_facts(expr, &exact_facts) {
            path_proof_cache_insert(&PATH_IMPLIES_CACHE, active, expr, result);
            emit_path_implies_event(
                active,
                expr,
                usize::from(result),
                profile_start,
                false,
                true,
                false,
                result,
                Some("exact_active_facts"),
            );
            return result;
        }
    }
    crate::profile::count(
        &PATH_IMPLIES_CACHE_MISSES,
        "math.proof.path_implies.cache_miss",
        |_| format!("active_size={} expr_size={}", active.size(), expr.size()),
    );
    if !crate::normalize::consume_proof_budget(proof_input_size, proof_input_size) {
        crate::profile::count(
            &PATH_IMPLIES_BUDGET_SKIPS,
            "math.proof.path_implies.budget_skip",
            |_| {
                format!(
                    "active_size={} expr_size={} reason=run_proof_budget",
                    active.size(),
                    expr.size()
                )
            },
        );
        emit_path_implies_event(
            active,
            expr,
            0,
            profile_start,
            false,
            true,
            true,
            false,
            Some("run_proof_budget"),
        );
        return false;
    }
    if structural_implication(active, expr, &mut 2048) {
        path_proof_cache_insert(&PATH_IMPLIES_CACHE, active, expr, true);
        emit_path_implies_event(
            active,
            expr,
            1,
            profile_start,
            false,
            true,
            false,
            true,
            None,
        );
        return true;
    }
    // Path implication is also an optimization proof. Avoid constructing a
    // second copy of very large merged guards for the arithmetic fallback;
    // failure to prove implication conservatively retains the branch.
    if active
        .size()
        .checked_add(expr.size())
        .is_none_or(|size| size > MAX_TRUTH_TABLE_INPUT_SIZE)
    {
        crate::profile::count(
            &PATH_IMPLIES_BUDGET_SKIPS,
            "math.proof.path_implies.budget_skip",
            |_| {
                format!(
                    "active_size={} expr_size={} size_budget={}",
                    active.size(),
                    expr.size(),
                    MAX_TRUTH_TABLE_INPUT_SIZE
                )
            },
        );
        path_proof_cache_insert(&PATH_IMPLIES_CACHE, active, expr, false);
        emit_path_implies_event(
            active,
            expr,
            0,
            profile_start,
            false,
            true,
            true,
            false,
            Some("input_size_budget"),
        );
        return false;
    }
    // Propositional validity is sufficient even when the atoms are arithmetic
    // comparisons. A counterexample is conclusive only for independent Boolean
    // symbols; correlated comparisons must still use the arithmetic prover.
    let conditions = [not(active.clone()), expr.clone()];
    match boolean_truth_table_covers_all_precheck(&conditions) {
        TruthTableCoverage::CoversAll => {
            path_proof_cache_insert(&PATH_IMPLIES_CACHE, active, expr, true);
            emit_path_implies_event(
                active,
                expr,
                1,
                profile_start,
                false,
                true,
                false,
                true,
                None,
            );
            return true;
        }
        TruthTableCoverage::DoesNotCoverAllExact => {
            path_proof_cache_insert(&PATH_IMPLIES_CACHE, active, expr, false);
            emit_path_implies_event(
                active,
                expr,
                0,
                profile_start,
                false,
                true,
                false,
                false,
                None,
            );
            return false;
        }
        TruthTableCoverage::Unknown => {}
    }
    if proof_can_skip_as_unrelated(active, expr) {
        crate::profile::count(
            &PATH_IMPLIES_UNRELATED_SKIPS,
            "math.proof.path_implies.unrelated_skip",
            |_| format!("active_size={} expr_size={}", active.size(), expr.size()),
        );
        path_proof_cache_insert(&PATH_IMPLIES_CACHE, active, expr, false);
        emit_path_implies_event(
            active,
            expr,
            0,
            profile_start,
            false,
            true,
            false,
            false,
            Some("unrelated_atoms"),
        );
        return false;
    }
    let counterexample_conditions = [active.clone(), not(expr.clone())];
    if boolean_conditions_have_independent_model(&counterexample_conditions) {
        crate::profile::count(
            &PATH_IMPLIES_INDEPENDENT_COUNTEREXAMPLES,
            "math.proof.path_implies.independent_counterexample",
            |_| format!("active_size={} expr_size={}", active.size(), expr.size()),
        );
        path_proof_cache_insert(&PATH_IMPLIES_CACHE, active, expr, false);
        emit_path_implies_event(
            active,
            expr,
            0,
            profile_start,
            false,
            true,
            false,
            false,
            Some("independent_counterexample"),
        );
        return false;
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
    path_proof_cache_insert(&PATH_IMPLIES_CACHE, active, expr, result);
    emit_path_implies_event(
        active,
        expr,
        usize::from(result),
        profile_start,
        false,
        true,
        false,
        result,
        None,
    );
    result
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TruthTableCoverage {
    CoversAll,
    DoesNotCoverAllExact,
    Unknown,
}

fn truth_table_input_within_budget(conds: &[Expr]) -> bool {
    conds
        .iter()
        .try_fold(0usize, |total, cond| total.checked_add(cond.size()))
        .is_some_and(|total| total <= MAX_TRUTH_TABLE_INPUT_SIZE)
}

fn boolean_truth_table_covers_all_precheck(conds: &[Expr]) -> TruthTableCoverage {
    if !truth_table_input_within_budget(conds) {
        return TruthTableCoverage::Unknown;
    }
    let mut atoms: Vec<String> = Vec::new();
    for cond in conds {
        collect_boolean_atoms(cond, &mut atoms);
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
    if let Some((op, operand)) = expr.as_unary_op() {
        if op == UnaryOpKind::Not {
            return CompiledBoolExpr::Not(Box::new(compile_boolean_truth_table_expr(
                operand, atom_index,
            )));
        }
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

    let key = format!("{:?}", expr);
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

fn collect_boolean_atom_exprs(expr: &Expr, atoms: &mut Vec<Expr>) {
    if expr.as_bool().is_some() {
        return;
    }
    if let Some((UnaryOpKind::Not, operand)) = expr.as_unary_op() {
        collect_boolean_atom_exprs(operand, atoms);
        return;
    }
    if let Some((op, left, right)) = expr.as_binop() {
        match op {
            BinOpKind::And | BinOpKind::Or => {
                collect_boolean_atom_exprs(left, atoms);
                collect_boolean_atom_exprs(right, atoms);
                return;
            }
            BinOpKind::Eq | BinOpKind::Ne => {
                atoms.push(expr.clone());
                return;
            }
            _ => {}
        }
    }
    atoms.push(expr.clone());
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
        collect_boolean_atom_exprs(condition, &mut atom_exprs);
    }
    let mut atoms = Vec::new();
    let mut atom_values = HashMap::<String, Expr>::new();
    for atom in atom_exprs {
        let key = if let Some((BinOpKind::Eq | BinOpKind::Ne, left, right)) = atom.as_binop() {
            comparison_atom_key(left, right)
        } else {
            format!("{:?}", atom)
        };
        if !atom_values.contains_key(&key) {
            atom_values.insert(key, atom.clone());
            atoms.push(atom);
        }
    }
    if atoms.len() > 32 {
        return false;
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
    if let Some((op, operand)) = expr.as_unary_op() {
        return op == UnaryOpKind::Not && is_pure_boolean_propositional_expr(operand);
    }
    if let Some((op, left, right)) = expr.as_binop() {
        return matches!(op, BinOpKind::And | BinOpKind::Or)
            && is_pure_boolean_propositional_expr(left)
            && is_pure_boolean_propositional_expr(right);
    }
    matches!(
        expr.kind(),
        ExprKind::SymVar {
            origin: SymVarOrigin::Parameter { primitive_type, .. }
                | SymVarOrigin::External { primitive_type, .. }
                | SymVarOrigin::Temporal { primitive_type, .. },
        } if primitive_type == "boolean"
    )
}

fn collect_boolean_atoms(expr: &Expr, atoms: &mut Vec<String>) {
    if expr.as_bool().is_some() {
        return;
    }
    if let Some((op, operand)) = expr.as_unary_op() {
        if op == UnaryOpKind::Not {
            collect_boolean_atoms(operand, atoms);
            return;
        }
    }
    if let Some((op, left, right)) = expr.as_binop() {
        match op {
            BinOpKind::And | BinOpKind::Or => {
                collect_boolean_atoms(left, atoms);
                collect_boolean_atoms(right, atoms);
                return;
            }
            BinOpKind::Eq | BinOpKind::Ne => {
                atoms.push(comparison_atom_key(left, right));
                return;
            }
            _ => {}
        }
    }
    atoms.push(format!("{:?}", expr));
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
/// # Panics
/// args 长度与 func 注册的 arity 不符。
pub fn call(key: FuncKey, args: Vec<Expr>) -> Expr {
    let declared = func_arity(key);
    assert_eq!(
        args.len(),
        declared,
        "[MATH-BUILDER] call arity mismatch: func expects {} args, got {}",
        declared,
        args.len()
    );
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
