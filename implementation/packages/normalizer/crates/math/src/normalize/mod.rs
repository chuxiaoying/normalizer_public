//! 化简入口 —— `normalize(expr) → expr` 是唯一对外 API。
//!
//! ## 设计
//!
//! - `normalize` 是单一入口
//! - 内部采用 **rebuild-with-children** 模式：先递归化简子表达式，再用原节点的身份/运算符 rebuild，再对自身做本地化简
//! - 不动点迭代：重复化简直到表达式不变；预算耗尽明确失败，不返回未完成结果
//! - 规则集私有，从 pub 面不可见
//!
//! ## 扩展规则
//!
//! 新增规则 = 修改本模块。外部无法注入规则（杜绝"散落的化简规则"）。

mod arithmetic;
mod binop_casewhen;
mod boolean;
mod boolean_casewhen;
mod casewhen;
mod constant;
pub(crate) mod constraint;
mod integer_ac;
mod integer_equations;
mod integer_ranges;
mod rebuild;
pub(crate) mod relation;
use crate::expr::{Expr, ExprKind};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::AtomicU64;
use std::time::Instant;

thread_local! {
    static DEFAULT_NORMALIZE_CACHE: RefCell<HashMap<Expr, Expr>> = RefCell::new(HashMap::new());
    static DEFAULT_SIMPLIFY_ONCE_CACHE: RefCell<HashMap<Expr, Expr>> = RefCell::new(HashMap::new());
    static NORMALIZE_RUN_CACHE_COUNTS: RefCell<(u64, u64)> = RefCell::new((0, 0));
    static ACTIVE_BUDGET_ABORT: RefCell<Option<NormalizationLimit>> = RefCell::new(None);
}

/// 最大迭代轮数（不动点保护）。
const MAX_ITERATIONS: usize = 100;
const DEFAULT_MAX_PROOF_NODES: usize = 32_768;

static NORMALIZE_CACHE_HITS: AtomicU64 = AtomicU64::new(0);
static NORMALIZE_CACHE_MISSES: AtomicU64 = AtomicU64::new(0);
static NORMALIZE_MEMO_HITS: AtomicU64 = AtomicU64::new(0);
static NORMALIZE_MEMO_MISSES: AtomicU64 = AtomicU64::new(0);
static NORMALIZE_SIMPLIFY_ONCE_CACHE_HITS: AtomicU64 = AtomicU64::new(0);
static NORMALIZE_SIMPLIFY_ONCE_CACHE_MISSES: AtomicU64 = AtomicU64::new(0);

/// The bounded work categories shared by normalization and proof helpers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BudgetKind {
    Steps,
    CandidateNodes,
    ProofNodes,
}

impl std::fmt::Display for BudgetKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Steps => "steps",
            Self::CandidateNodes => "candidate_nodes",
            Self::ProofNodes => "proof_nodes",
        })
    }
}

/// Per-root workload accounting. The defaults preserve the existing iteration
/// and proof guards while making the aggregate work visible and configurable.
#[derive(Clone, Debug)]
struct NormalizeBudget {
    pub max_steps: usize,
    pub max_candidate_nodes: usize,
    pub max_proof_nodes: usize,
    pub steps: usize,
    pub rewrites: usize,
    pub candidate_nodes: usize,
    pub proof_nodes: usize,
    pub peak_nodes: usize,
    pub budget_skips: usize,
    pub last_skip: Option<BudgetKind>,
    pub exhausted: Option<BudgetKind>,
}

impl Default for NormalizeBudget {
    fn default() -> Self {
        Self {
            max_steps: env_budget("NORMALIZER_MATH_MAX_STEPS", MAX_ITERATIONS * 100_000),
            max_candidate_nodes: env_budget("NORMALIZER_MATH_MAX_CANDIDATE_NODES", 4_000_000),
            max_proof_nodes: env_budget("NORMALIZER_MATH_MAX_PROOF_NODES", DEFAULT_MAX_PROOF_NODES),
            steps: 0,
            rewrites: 0,
            candidate_nodes: 0,
            proof_nodes: 0,
            peak_nodes: 0,
            budget_skips: 0,
            last_skip: None,
            exhausted: None,
        }
    }
}

impl NormalizeBudget {
    fn consume(&mut self, kind: BudgetKind, amount: usize, observed_nodes: usize) -> bool {
        self.peak_nodes = self.peak_nodes.max(observed_nodes);
        let (used, limit) = match kind {
            BudgetKind::Steps => (&mut self.steps, self.max_steps),
            BudgetKind::CandidateNodes => (&mut self.candidate_nodes, self.max_candidate_nodes),
            BudgetKind::ProofNodes => (&mut self.proof_nodes, self.max_proof_nodes),
        };
        if amount > limit.saturating_sub(*used) {
            self.budget_skips = self.budget_skips.saturating_add(1);
            self.last_skip = Some(kind);
            if kind == BudgetKind::Steps {
                self.exhausted = Some(kind);
            }
            return false;
        }
        *used += amount;
        true
    }
}

fn env_budget(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(default)
}

thread_local! {
    static ACTIVE_BUDGET: RefCell<Option<NormalizeBudget>> = RefCell::new(None);
}

struct BudgetScope {
    armed: bool,
}

impl BudgetScope {
    fn install(budget: NormalizeBudget) -> Self {
        ACTIVE_BUDGET.with(|slot| slot.replace(Some(budget)));
        Self { armed: true }
    }

    fn take(mut self) -> NormalizeBudget {
        self.armed = false;
        ACTIVE_BUDGET.with(|slot| {
            slot.replace(None)
                .expect("normalization budget scope missing")
        })
    }
}

impl Drop for BudgetScope {
    fn drop(&mut self) {
        if self.armed {
            ACTIVE_BUDGET.with(|slot| {
                slot.replace(None);
            });
        }
    }
}

pub(crate) fn consume_proof_budget(amount: usize, input_nodes: usize) -> bool {
    ACTIVE_BUDGET.with(|budget| {
        let mut slot = budget.borrow_mut();
        let Some(budget) = slot.as_mut() else {
            return true;
        };
        budget.consume(BudgetKind::ProofNodes, amount, input_nodes)
    })
}

pub(crate) fn consume_candidate_budget(amount: usize, output_nodes: usize) -> bool {
    ACTIVE_BUDGET.with(|budget| {
        let mut slot = budget.borrow_mut();
        let Some(budget) = slot.as_mut() else {
            return true;
        };
        budget.consume(BudgetKind::CandidateNodes, amount, output_nodes)
    })
}

fn budget_snapshot() -> Option<NormalizeBudget> {
    ACTIVE_BUDGET.with(|budget| budget.borrow().clone())
}

pub(crate) fn budget_remaining() -> (usize, usize, usize) {
    budget_snapshot().map_or((0, 0, 0), |budget| {
        (
            budget.max_steps.saturating_sub(budget.steps),
            budget
                .max_candidate_nodes
                .saturating_sub(budget.candidate_nodes),
            budget.max_proof_nodes.saturating_sub(budget.proof_nodes),
        )
    })
}

fn run_with_budget<F>(run: F) -> Result<Expr, NormalizationLimit>
where
    F: FnOnce() -> Result<Expr, NormalizationLimit>,
{
    ACTIVE_BUDGET.with(|slot| {
        if slot.borrow().is_some() {
            return run();
        }
        let run_scope = crate::profile::begin_run();
        let scope = BudgetScope::install(NormalizeBudget::default());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(run));
        let budget = scope.take();
        let deferred_error = ACTIVE_BUDGET_ABORT.with(|abort| abort.borrow_mut().take());
        drop(run_scope);
        let result = match result {
            Ok(result) => result,
            Err(payload) => std::panic::resume_unwind(payload),
        };
        if let Some(error) = deferred_error {
            return Err(error);
        }
        if let Some(kind) = budget.exhausted {
            let remaining_size = result
                .as_ref()
                .map_or_else(|error| error.remaining_size, Expr::size);
            return Err(NormalizationLimit {
                iterations: budget.steps.min(MAX_ITERATIONS),
                remaining_size,
                budget_kind: Some(kind),
                peak_intermediate_size: budget.peak_nodes,
            });
        }
        result
    })
}

fn normalize_run_cache_counts() -> (u64, u64) {
    NORMALIZE_RUN_CACHE_COUNTS.with(|counts| *counts.borrow())
}

fn record_normalize_cache_hit() {
    NORMALIZE_RUN_CACHE_COUNTS.with(|counts| counts.borrow_mut().0 += 1);
}

fn record_normalize_cache_miss() {
    NORMALIZE_RUN_CACHE_COUNTS.with(|counts| counts.borrow_mut().1 += 1);
}

/// Resource exhaustion is not a normal form or a proof of false.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NormalizationLimit {
    pub iterations: usize,
    pub remaining_size: usize,
    pub budget_kind: Option<BudgetKind>,
    pub peak_intermediate_size: usize,
}

impl std::fmt::Display for NormalizationLimit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(kind) = self.budget_kind {
            write!(f, "normalization {kind} budget exhausted after {} steps (remaining expression size {}, peak intermediate size {}); no canonical result was produced", self.iterations, self.remaining_size, self.peak_intermediate_size)
        } else {
            write!(f, "normalization iteration budget exhausted after {} steps (remaining expression size {}); no canonical result was produced", self.iterations, self.remaining_size)
        }
    }
}
impl std::error::Error for NormalizationLimit {}

/// 归一化 / 化简（带 SortContext，IoC）。
///
/// 反复应用化简规则直到不动点（最多 MAX_ITERATIONS 轮）。
pub fn normalize_with(expr: Expr, ctx: &dyn crate::sort_context::SortContext) -> Expr {
    try_normalize_with(expr, ctx).unwrap_or_else(|error| panic!("{error}"))
}

/// Checked normalization for callers that need to report resource failure.
pub fn try_normalize_with(
    expr: Expr,
    ctx: &dyn crate::sort_context::SortContext,
) -> Result<Expr, NormalizationLimit> {
    run_with_budget(|| normalize_with_impl(expr, ctx, false, MAX_ITERATIONS))
}

fn normalize_with_impl(
    expr: Expr,
    ctx: &dyn crate::sort_context::SortContext,
    use_default_simplify_once_cache: bool,
    iteration_limit: usize,
) -> Result<Expr, NormalizationLimit> {
    if is_normalize_atom(&expr) {
        return Ok(expr);
    }

    let profile_enabled = crate::profile::enabled();
    let profile_start = profile_enabled.then(Instant::now);
    let input_size = profile_enabled.then(|| expr.size());
    let input_depth = profile_enabled.then(|| expr.depth());
    let cache_counts_before = normalize_run_cache_counts();
    let evictions_before = crate::profile::cache_evictions();

    // Expr is immutable and simplify_once is pure for a fixed SortContext.
    // Reuse one memo across the whole fixed-point run so stable subtrees seen in
    // earlier iterations are not rebuilt/re-simplified again in later iterations.
    let memo = RefCell::new(HashMap::new());
    let mut current = expr;
    for iteration in 0..iteration_limit {
        let step_allowed = ACTIVE_BUDGET.with(|budget| {
            budget
                .borrow_mut()
                .as_mut()
                .is_none_or(|budget| budget.consume(BudgetKind::Steps, 1, current.size()))
        });
        if !step_allowed {
            let budget = budget_snapshot().expect("active normalization budget missing");
            return Err(NormalizationLimit {
                iterations: iteration,
                remaining_size: current.size(),
                budget_kind: budget.exhausted,
                peak_intermediate_size: budget.peak_nodes,
            });
        }
        let next = simplify_once_memo(&current, ctx, &memo, use_default_simplify_once_cache);
        if next == current {
            if let Some(start) = profile_start {
                crate::profile::event_elapsed_if_slow(
                    "math.normalize",
                    &format!(
                        "iterations={} input_size={} input_depth={} output_size={} output_depth={}",
                        iteration + 1,
                        input_size.unwrap_or(0),
                        input_depth.unwrap_or(0),
                        current.size(),
                        current.depth()
                    ),
                    start,
                );
            }
            emit_summary(
                input_size.unwrap_or(0),
                input_depth.unwrap_or(0),
                &current,
                iteration + 1,
                profile_start.map(|start| start.elapsed().as_micros()),
                cache_counts_before,
                evictions_before,
            );
            return Ok(current);
        }
        ACTIVE_BUDGET.with(|budget| {
            if let Some(budget) = budget.borrow_mut().as_mut() {
                budget.rewrites = budget.rewrites.saturating_add(1);
            }
        });
        current = next;
    }
    if let Some(start) = profile_start {
        crate::profile::event_elapsed_if_slow(
            "math.normalize",
            &format!(
                "iterations=max input_size={} input_depth={} output_size={} output_depth={}",
                input_size.unwrap_or(0),
                input_depth.unwrap_or(0),
                current.size(),
                current.depth()
            ),
            start,
        );
    }
    emit_summary(
        input_size.unwrap_or(0),
        input_depth.unwrap_or(0),
        &current,
        iteration_limit,
        profile_start.map(|start| start.elapsed().as_micros()),
        cache_counts_before,
        evictions_before,
    );
    Err(NormalizationLimit {
        iterations: iteration_limit,
        remaining_size: current.size(),
        budget_kind: None,
        peak_intermediate_size: budget_snapshot()
            .map_or(current.size(), |budget| budget.peak_nodes),
    })
}

fn emit_summary(
    input_size: usize,
    input_depth: usize,
    output: &Expr,
    iterations: usize,
    elapsed_us: Option<u128>,
    cache_counts_before: (u64, u64),
    evictions_before: u64,
) {
    let Some(budget) = budget_snapshot() else {
        return;
    };
    crate::profile::json_event(
        "math.normalize.root",
        &[
            ("phase", "normalize".to_string().quote_json()),
            ("rule", "fixed_point".to_string().quote_json()),
            ("input_size", input_size.to_string()),
            ("input_depth", input_depth.to_string()),
            ("output_size", output.size().to_string()),
            ("output_depth", output.depth().to_string()),
            ("iteration", iterations.to_string()),
            ("rewrite_count", budget.rewrites.to_string()),
            ("elapsed_us", elapsed_us.unwrap_or(0).to_string()),
            ("steps", budget.steps.to_string()),
            ("candidate_nodes", budget.candidate_nodes.to_string()),
            ("proof_nodes", budget.proof_nodes.to_string()),
            (
                "budget_remaining",
                budget.max_steps.saturating_sub(budget.steps).to_string(),
            ),
            (
                "candidate_budget_remaining",
                budget
                    .max_candidate_nodes
                    .saturating_sub(budget.candidate_nodes)
                    .to_string(),
            ),
            (
                "proof_budget_remaining",
                budget
                    .max_proof_nodes
                    .saturating_sub(budget.proof_nodes)
                    .to_string(),
            ),
            (
                "cache_hit",
                normalize_run_cache_counts()
                    .0
                    .saturating_sub(cache_counts_before.0)
                    .to_string(),
            ),
            (
                "cache_miss",
                normalize_run_cache_counts()
                    .1
                    .saturating_sub(cache_counts_before.1)
                    .to_string(),
            ),
            (
                "cache_eviction",
                crate::profile::cache_evictions()
                    .saturating_sub(evictions_before)
                    .to_string(),
            ),
            ("budget_skip", budget.budget_skips.to_string()),
            (
                "budget_kind",
                budget
                    .last_skip
                    .or(budget.exhausted)
                    .map_or_else(|| "null".to_string(), |kind| kind.to_string().quote_json()),
            ),
            (
                "skip_reason",
                if budget.budget_skips == 0 {
                    "null".to_string()
                } else {
                    "budget_exhausted".to_string().quote_json()
                },
            ),
            ("peak_intermediate_size", budget.peak_nodes.to_string()),
        ],
    );
}

trait JsonValue {
    fn quote_json(self) -> String;
}

impl JsonValue for String {
    fn quote_json(self) -> String {
        let mut value = String::with_capacity(self.len() + 2);
        value.push('"');
        for ch in self.chars() {
            match ch {
                '"' => value.push_str("\\\""),
                '\\' => value.push_str("\\\\"),
                _ => value.push(ch),
            }
        }
        value.push('"');
        value
    }
}

/// 归一化 / 化简（便捷版，使用 DefaultSortContext）。
pub fn normalize(expr: Expr) -> Expr {
    if ACTIVE_BUDGET_ABORT.with(|abort| abort.borrow().is_some()) {
        return expr;
    }
    if is_normalize_atom(&expr) {
        return expr;
    }

    let key = expr.clone();
    if let Some(cached) = DEFAULT_NORMALIZE_CACHE.with(|cache| cache.borrow().get(&key).cloned()) {
        if crate::profile::cache_observation_enabled() {
            crate::profile::cache_lookup(
                "normalize",
                "normalize",
                true,
                expr.structural_hash(),
                expr.size(),
                "fixed_point",
            );
        }
        crate::profile::count(&NORMALIZE_CACHE_HITS, "math.normalize.cache_hit", |_| {
            format!("size={} depth={}", expr.size(), expr.depth())
        });
        record_normalize_cache_hit();
        return cached;
    }
    if crate::profile::cache_observation_enabled() {
        crate::profile::cache_lookup(
            "normalize",
            "normalize",
            false,
            expr.structural_hash(),
            expr.size(),
            "fixed_point",
        );
    }
    crate::profile::count(&NORMALIZE_CACHE_MISSES, "math.normalize.cache_miss", |_| {
        format!("size={} depth={}", expr.size(), expr.depth())
    });
    record_normalize_cache_miss();

    let out = match run_with_budget(|| {
        normalize_with_impl(
            expr,
            &crate::sort_context::DefaultSortContext,
            true,
            MAX_ITERATIONS,
        )
    }) {
        Ok(out) => out,
        Err(error) => {
            let nested = ACTIVE_BUDGET.with(|budget| budget.borrow().is_some());
            if nested {
                ACTIVE_BUDGET_ABORT.with(|abort| {
                    let mut abort = abort.borrow_mut();
                    if abort.is_none() {
                        *abort = Some(error);
                    }
                });
                return key;
            }
            panic!("{error}");
        }
    };
    let cache_weight = key.size().saturating_add(out.size());
    if cache_weight <= crate::profile::cache_entry_limit() {
        DEFAULT_NORMALIZE_CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();
            crate::cache::insert_bounded(&mut cache, key, out.clone(), crate::cache::DEFAULT_KEY_LIMIT, "normalize");
        });
    } else {
        crate::profile::cache_admission_skip("normalize", cache_weight);
    }
    out
}

/// 单次完整化简：rebuild-with-children + simplify_self。
fn simplify_once_memo(
    expr: &Expr,
    ctx: &dyn crate::sort_context::SortContext,
    memo: &RefCell<HashMap<Expr, Expr>>,
    use_default_simplify_once_cache: bool,
) -> Expr {
    if is_normalize_atom(expr) {
        return expr.clone();
    }

    let key = expr.clone();
    if let Some(cached) = memo.borrow().get(&key).cloned() {
        if crate::profile::cache_observation_enabled() {
            crate::profile::cache_lookup(
                "normalize",
                "memo",
                true,
                expr.structural_hash(),
                expr.size(),
                "simplify_once",
            );
        }
        crate::profile::count(&NORMALIZE_MEMO_HITS, "math.normalize.memo_hit", |_| {
            format!("size={} depth={}", expr.size(), expr.depth())
        });
        return cached;
    }
    if crate::profile::cache_observation_enabled() {
        crate::profile::cache_lookup(
            "normalize",
            "memo",
            false,
            expr.structural_hash(),
            expr.size(),
            "simplify_once",
        );
    }

    if use_default_simplify_once_cache {
        if let Some(cached) = default_simplify_once_cache_get(&key) {
            if crate::profile::cache_observation_enabled() {
                crate::profile::cache_lookup(
                    "normalize",
                    "simplify_once",
                    true,
                    expr.structural_hash(),
                    expr.size(),
                    "simplify_once",
                );
            }
            crate::profile::count(
                &NORMALIZE_SIMPLIFY_ONCE_CACHE_HITS,
                "math.normalize.simplify_once_cache_hit",
                |_| format!("size={} depth={}", expr.size(), expr.depth()),
            );
            memo.borrow_mut().insert(key, cached.clone());
            return cached;
        }
        if crate::profile::cache_observation_enabled() {
            crate::profile::cache_lookup(
                "normalize",
                "simplify_once",
                false,
                expr.structural_hash(),
                expr.size(),
                "simplify_once",
            );
        }
        crate::profile::count(
            &NORMALIZE_SIMPLIFY_ONCE_CACHE_MISSES,
            "math.normalize.simplify_once_cache_miss",
            |_| format!("size={} depth={}", expr.size(), expr.depth()),
        );
    }

    crate::profile::count(&NORMALIZE_MEMO_MISSES, "math.normalize.memo_miss", |_| {
        format!("size={} depth={}", expr.size(), expr.depth())
    });

    // 先递归化简子表达式
    let closure = |e: &Expr| simplify_once_memo(e, ctx, memo, use_default_simplify_once_cache);
    let with_simplified_children = rebuild::rebuild_with_children_simplified(expr, &closure);
    // 再对自身做本地化简
    let out = simplify_self(&with_simplified_children, ctx);
    let _ = consume_candidate_budget(out.size(), out.size());
    memo.borrow_mut().insert(key.clone(), out.clone());
    if use_default_simplify_once_cache {
        default_simplify_once_cache_insert(key, &out);
    }
    out
}

fn default_simplify_once_cache_get(key: &Expr) -> Option<Expr> {
    DEFAULT_SIMPLIFY_ONCE_CACHE.with(|cache| cache.borrow().get(key).cloned())
}

fn default_simplify_once_cache_insert(key: Expr, output: &Expr) {
    let cache_weight = key.size().saturating_add(output.size());
    if cache_weight > crate::profile::cache_entry_limit() {
        crate::profile::cache_admission_skip("simplify_once", cache_weight);
        return;
    }
    DEFAULT_SIMPLIFY_ONCE_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        crate::cache::insert_bounded(&mut cache, key, output.clone(), crate::cache::DEFAULT_KEY_LIMIT, "simplify_once");
    });
}

fn is_normalize_atom(expr: &Expr) -> bool {
    matches!(
        expr.kind(),
        ExprKind::Lit(_) | ExprKind::SymVar { .. } | ExprKind::HeapRef(_)
    )
}

/// 本地化简：只对顶层节点应用规则，不递归。
pub(crate) fn simplify_self(expr: &Expr, ctx: &dyn crate::sort_context::SortContext) -> Expr {
    // 顺序：常量折叠 → 算术代数 → 布尔代数 → CaseWhen 化简
    //
    // 常量折叠必须先于 arithmetic：例如 `(3 - 2) > 0` 应先折叠为 `true`，
    // 而不是先做 `(x - y) > 0 → x > y` 的符号形变。
    let e = constant::fold(expr);
    let e = arithmetic::simplify(&e);
    let e = integer_ac::simplify(&e, ctx);
    let e = crate::canonical_order::orient_ordered_expression(&e, ctx);
    let e = boolean::simplify(&e, ctx);
    let e = binop_casewhen::simplify(&e);
    let e = boolean_casewhen::simplify(&e);
    let e = relation::simplify(&e);
    let e = casewhen::simplify(&e, ctx);
    e
}
