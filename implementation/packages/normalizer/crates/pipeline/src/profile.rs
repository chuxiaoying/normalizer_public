//! 实时内核的可选 profile 事件出口。
//!
//! 只保留新 pipeline 实际使用的稳定接口。旧执行器的 scope、暂停看门狗和
//! 全量状态快照不属于实时归一化内核，因此不再通过 profile 反向保留。

use std::fs::OpenOptions;
use std::io::Write;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

static PROFILE_LOG_PATH: OnceLock<Option<String>> = OnceLock::new();
static PROFILE_LOG_FILE: OnceLock<Option<Mutex<std::fs::File>>> = OnceLock::new();
static DEBUG_CATEGORIES: OnceLock<Option<Vec<String>>> = OnceLock::new();
static MAX_EVENTS: OnceLock<Option<u64>> = OnceLock::new();
static EVENT_COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static RUN_ID: OnceLock<String> = OnceLock::new();

fn run_id() -> &'static str {
    RUN_ID
        .get_or_init(|| {
            std::env::var("NORMALIZER_RUN_ID").unwrap_or_else(|_| {
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|duration| duration.as_micros().to_string())
                    .unwrap_or_else(|_| "unknown".to_string())
            })
        })
        .as_str()
}

fn profile_log_path() -> Option<&'static str> {
    PROFILE_LOG_PATH
        .get_or_init(|| {
            std::env::var("NORMALIZER_PROFILE_LOG")
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
        .as_deref()
}

fn profile_log_file() -> Option<&'static Mutex<std::fs::File>> {
    PROFILE_LOG_FILE
        .get_or_init(|| {
            let path = profile_log_path()?;
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .ok()
                .map(Mutex::new)
        })
        .as_ref()
}

fn debug_categories() -> Option<&'static [String]> {
    DEBUG_CATEGORIES
        .get_or_init(|| {
            let value = std::env::var("NORMALIZER_DEBUG_CATEGORIES").ok()?;
            let categories = value
                .split(',')
                .map(str::trim)
                .filter(|category| !category.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>();
            (!categories.is_empty()).then_some(categories)
        })
        .as_deref()
}

fn max_events() -> Option<u64> {
    *MAX_EVENTS.get_or_init(|| {
        std::env::var("NORMALIZER_DEBUG_MAX_EVENTS")
            .ok()?
            .parse()
            .ok()
    })
}

fn category_enabled(category: &str) -> bool {
    debug_categories()
        .map(|categories| {
            categories.iter().any(|selected| {
                selected == "*"
                    || category == selected
                    || category.starts_with(&format!("{selected}."))
            })
        })
        .unwrap_or(true)
}

pub(crate) fn enabled() -> bool {
    profile_log_path().is_some() || debug_categories().is_some()
}

pub(crate) fn selected(category: &str) -> bool {
    enabled() && category_enabled(category)
}

pub(crate) fn event(category: &str, message: &str) {
    if !category_enabled(category) {
        return;
    }
    let sequence = EVENT_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
    if max_events().is_some_and(|limit| sequence > limit) {
        return;
    }
    let Some(file) = profile_log_file() else {
        eprintln!("[normalizer:{category}#{sequence}] {message}");
        return;
    };
    let timestamp_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    let mut file = file.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let _ = writeln!(
        file,
        "schema=normalizer.profile/1 run_id={} seq={} ts_ms={} category={} {}",
        run_id(),
        sequence,
        timestamp_ms,
        category,
        message.replace('\n', "\\n")
    );
}
