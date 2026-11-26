use std::env;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::Instant;

static PROFILE_PATH: OnceLock<Option<PathBuf>> = OnceLock::new();
static CONFIGURED_RUN_ID: OnceLock<Option<String>> = OnceLock::new();
static RUN_SEQUENCE: AtomicU64 = AtomicU64::new(1);
static STRUCTURED_PROFILE: OnceLock<bool> = OnceLock::new();
static CACHE_ENTRY_LIMIT: OnceLock<usize> = OnceLock::new();
static CACHE_EVICTIONS: AtomicU64 = AtomicU64::new(0);
static CACHE_LOOKUPS: AtomicU64 = AtomicU64::new(0);

thread_local! {
    static ACTIVE_RUN_ID: std::cell::RefCell<Option<String>> = std::cell::RefCell::new(None);
    static CACHE_EVICTIONS_LOCAL: std::cell::Cell<u64> = std::cell::Cell::new(0);
}

fn profile_path() -> Option<&'static PathBuf> {
    PROFILE_PATH
        .get_or_init(|| {
            let explicit = env::var("NORMALIZER_MATH_PROFILE_LOG").ok();
            let enabled = env::var("NORMALIZER_MATH_PROFILE")
                .ok()
                .map(|v| v != "0" && !v.eq_ignore_ascii_case("false"))
                .unwrap_or(false);
            explicit
                .or_else(|| {
                    enabled
                        .then(|| env::var("NORMALIZER_PROFILE_LOG").ok())
                        .flatten()
                })
                .map(PathBuf::from)
        })
        .as_ref()
}

pub(crate) fn enabled() -> bool {
    profile_path().is_some()
}

pub(crate) struct RunScope(());

pub(crate) fn begin_run() -> RunScope {
    let configured = CONFIGURED_RUN_ID.get_or_init(|| {
        env::var("NORMALIZER_MATH_PROFILE_RUN_ID")
            .or_else(|_| env::var("NORMALIZER_DEBUG_RUN_ID"))
            .ok()
    });
    let run_id = configured.clone().unwrap_or_else(|| {
        format!(
            "pid-{}-thread-{:?}-run-{}",
            std::process::id(),
            std::thread::current().id(),
            RUN_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        )
    });
    ACTIVE_RUN_ID.with(|active| active.replace(Some(run_id)));
    RunScope(())
}

impl Drop for RunScope {
    fn drop(&mut self) {
        ACTIVE_RUN_ID.with(|active| active.replace(None));
    }
}

pub(crate) fn run_id() -> String {
    if let Some(run_id) = ACTIVE_RUN_ID.with(|active| active.borrow().clone()) {
        return run_id;
    }
    CONFIGURED_RUN_ID
        .get_or_init(|| {
            env::var("NORMALIZER_MATH_PROFILE_RUN_ID")
                .or_else(|_| env::var("NORMALIZER_DEBUG_RUN_ID"))
                .ok()
        })
        .clone()
        .unwrap_or_else(|| "unscoped-profile".to_string())
}

fn slow_threshold_ms() -> u128 {
    env::var("NORMALIZER_MATH_PROFILE_SLOW_MS")
        .ok()
        .and_then(|v| v.parse::<u128>().ok())
        .unwrap_or(10)
}

pub(crate) fn event(category: &str, detail: &str) {
    if structured_enabled() {
        return;
    }
    let Some(path) = profile_path() else {
        return;
    };
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{} {}", category, detail);
    }
}

/// Emit a compact JSON object when the opt-in structured profile mode is on.
/// The normal text profile remains the default for compatibility with existing
/// tooling. Keeping this writer dependency-free also keeps the math crate small.
pub(crate) fn json_event(category: &str, fields: &[(&str, String)]) {
    if !structured_enabled() {
        return;
    }
    let mut line = String::from("{\"event\":");
    line.push_str(&json_string(category));
    line.push_str(",\"run_id\":");
    line.push_str(&json_string(&run_id()));
    for (key, value) in fields {
        line.push(',');
        line.push_str(&json_string(key));
        line.push(':');
        line.push_str(value);
    }
    line.push('}');
    if let Some(path) = profile_path() {
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(file, "{line}");
        }
    }
}

/// Emit one opt-in rule-level event with the same accounting fields as the
/// root normalization summary. Rule callers only provide their local sizes
/// and a stable skip reason. The active normalization budget is sampled here
/// so every rule uses the same remaining-work definition.
pub(crate) fn rule_event(
    category: &str,
    input_size: usize,
    output_size: usize,
    elapsed_us: u128,
    skip_reason: Option<&str>,
    extra: &[(&str, String)],
) {
    if !enabled() {
        return;
    }
    let (budget_remaining, candidate_budget_remaining, proof_budget_remaining) =
        crate::normalize::budget_remaining();
    let skip_json = skip_reason
        .map(json_string)
        .unwrap_or_else(|| "null".to_string());
    let mut fields = vec![
        ("phase", json_string("normalize")),
        ("rule", json_string(category)),
        ("input_size", input_size.to_string()),
        ("output_size", output_size.to_string()),
        ("elapsed_us", elapsed_us.to_string()),
        ("budget_remaining", budget_remaining.to_string()),
        (
            "candidate_budget_remaining",
            candidate_budget_remaining.to_string(),
        ),
        ("proof_budget_remaining", proof_budget_remaining.to_string()),
        ("skip_reason", skip_json.clone()),
    ];
    fields.extend(extra.iter().map(|(key, value)| (*key, value.clone())));
    json_event(category, &fields);
    event(
        category,
        &format!(
            "input_size={} output_size={} elapsed_us={} budget_remaining={} skip_reason={}",
            input_size,
            output_size,
            elapsed_us,
            budget_remaining,
            skip_reason.unwrap_or("null")
        ),
    );
}

fn structured_enabled() -> bool {
    *STRUCTURED_PROFILE.get_or_init(|| {
        env::var("NORMALIZER_MATH_PROFILE_JSONL")
            .ok()
            .map(|value| value != "0" && !value.eq_ignore_ascii_case("false"))
            .unwrap_or(false)
    })
}

pub(crate) fn cache_observation_enabled() -> bool {
    structured_enabled()
}

pub(crate) fn cache_entry_limit() -> usize {
    *CACHE_ENTRY_LIMIT.get_or_init(|| {
        env::var("NORMALIZER_MATH_MAX_CACHE_ENTRY_NODES")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(65_536)
    })
}

pub(crate) fn cache_admission_skip(category: &str, input_size: usize) {
    json_event(
        "math.cache.skip",
        &[
            ("phase", json_string("cache")),
            ("scope", json_string("cache")),
            ("cache_kind", json_string(category)),
            ("action", json_string("admission_skip")),
            ("skip_reason", json_string("entry_weight_limit")),
            ("input_size", input_size.to_string()),
            ("entry_limit", cache_entry_limit().to_string()),
        ],
    );
}

pub(crate) fn json_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('"');
    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            _ => escaped.push(ch),
        }
    }
    escaped.push('"');
    escaped
}

pub(crate) fn cache_eviction(category: &str, before: usize, removed: usize, after: usize) {
    let total = CACHE_EVICTIONS.fetch_add(1, Ordering::Relaxed) + 1;
    CACHE_EVICTIONS_LOCAL.with(|count| count.set(count.get().saturating_add(1)));
    if enabled() {
        event(
            "math.cache.evict",
            &format!(
                "category={category} before={before} removed={removed} after={after} total={total}"
            ),
        );
    }
    json_event(
        "math.cache.evict",
        &[
            ("phase", json_string(cache_scope(category))),
            ("scope", json_string(cache_scope(category))),
            ("cache_kind", json_string(category)),
            ("action", json_string("eviction")),
            ("category", json_string(category)),
            ("before", before.to_string()),
            ("removed", removed.to_string()),
            ("after", after.to_string()),
            ("total", total.to_string()),
        ],
    );
}

/// Emit a replayable cache lookup record in structured profile mode.
///
/// Lookup events are deliberately JSONL-only. The text profile retains its
/// low-volume counter records, while JSONL consumers can correlate each cache
/// decision with the expression key and the active workload budget.
pub(crate) fn cache_lookup(
    scope: &str,
    cache_kind: &str,
    hit: bool,
    input_structural_hash: u64,
    input_size: usize,
    rule: &str,
) {
    if !structured_enabled() {
        return;
    }
    let lookup_count = CACHE_LOOKUPS.fetch_add(1, Ordering::Relaxed) + 1;
    let sample_all = env::var("NORMALIZER_MATH_PROFILE_CACHE_LOOKUPS")
        .ok()
        .is_some_and(|value| {
            value == "1" || value.eq_ignore_ascii_case("true") || value.eq_ignore_ascii_case("all")
        });
    // Full lookup tracing is available for replay investigations, while the
    // default structured profile emits only power-of-two samples to keep large
    // normalization runs bounded in log volume.
    if !sample_all && !lookup_count.is_power_of_two() {
        return;
    }
    let (budget_remaining, candidate_budget_remaining, proof_budget_remaining) =
        crate::normalize::budget_remaining();
    json_event(
        "math.cache.lookup",
        &[
            ("phase", json_string(scope)),
            ("scope", json_string(scope)),
            ("cache_kind", json_string(cache_kind)),
            ("action", json_string("lookup")),
            ("status", json_string(if hit { "hit" } else { "miss" })),
            ("sampled", (!sample_all).to_string()),
            ("lookup_count", lookup_count.to_string()),
            ("hit", usize::from(hit).to_string()),
            ("miss", usize::from(!hit).to_string()),
            ("input_structural_hash", input_structural_hash.to_string()),
            ("input_size", input_size.to_string()),
            ("rule", json_string(rule)),
            ("budget_remaining", budget_remaining.to_string()),
            (
                "candidate_budget_remaining",
                candidate_budget_remaining.to_string(),
            ),
            ("proof_budget_remaining", proof_budget_remaining.to_string()),
        ],
    );
}

fn cache_scope(category: &str) -> &str {
    if category.starts_with("path_") || category.starts_with("conditions_cover") {
        "proof"
    } else if category == "ordering_axiom" {
        "ordering"
    } else {
        "normalize"
    }
}

pub(crate) fn cache_evictions() -> u64 {
    CACHE_EVICTIONS_LOCAL.with(std::cell::Cell::get)
}

pub(crate) fn event_elapsed_if_slow(category: &str, detail: &str, start: Instant) {
    let elapsed_ms = start.elapsed().as_millis();
    if elapsed_ms >= slow_threshold_ms() {
        event(category, &format!("elapsed_ms={} {}", elapsed_ms, detail));
    }
}

pub(crate) fn count<F>(counter: &'static AtomicU64, category: &str, detail: F)
where
    F: FnOnce(u64) -> String,
{
    if !enabled() {
        return;
    }
    let n = counter.fetch_add(1, Ordering::Relaxed) + 1;
    if n.is_power_of_two() || n % 10_000 == 0 {
        event(category, &format!("count={} {}", n, detail(n)));
    }
}
