use std::env;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::Instant;

static PROFILE_PATH: OnceLock<Option<PathBuf>> = OnceLock::new();
static CACHE_ENTRY_LIMIT: OnceLock<usize> = OnceLock::new();
static CACHE_EVICTIONS: AtomicU64 = AtomicU64::new(0);

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

fn slow_threshold_ms() -> u128 {
    env::var("NORMALIZER_MATH_PROFILE_SLOW_MS")
        .ok()
        .and_then(|v| v.parse::<u128>().ok())
        .unwrap_or(10)
}

pub(crate) fn event(category: &str, detail: &str) {
    let Some(path) = profile_path() else {
        return;
    };
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{} {}", category, detail);
    }
}

/// Emit sizes, duration, skip reason and remaining steps for the text profile.
pub(crate) fn rule_event(
    category: &str,
    input_size: usize,
    output_size: usize,
    elapsed_us: u128,
    skip_reason: Option<&str>,
) {
    if !enabled() {
        return;
    }
    let budget_remaining = crate::normalize::remaining_steps();
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

pub(crate) fn cache_entry_limit() -> usize {
    *CACHE_ENTRY_LIMIT.get_or_init(|| {
        env::var("NORMALIZER_MATH_MAX_CACHE_ENTRY_NODES")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(65_536)
    })
}

pub(crate) fn cache_eviction(category: &str, before: usize, removed: usize, after: usize) {
    let total = CACHE_EVICTIONS.fetch_add(1, Ordering::Relaxed) + 1;
    if enabled() {
        event(
            "math.cache.evict",
            &format!(
                "category={category} before={before} removed={removed} after={after} total={total}"
            ),
        );
    }
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
