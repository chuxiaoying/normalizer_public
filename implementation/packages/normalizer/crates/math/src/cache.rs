//! Bounded retention for the math kernel's pure memo tables.
use std::{collections::HashMap, hash::Hash};

pub(crate) const DEFAULT_KEY_LIMIT: usize = 100_000;

pub(crate) fn insert_bounded<K: Eq + Hash + Clone, V>(
    cache: &mut HashMap<K, V>, key: K, value: V, limit: usize, category: &str,
) {
    // Every cache stores a pure result. Which keys are dropped cannot affect
    // the result of normalization, only whether a value must be recomputed.
    let before = cache.len();
    if before >= limit {
        let remove = (before / 2).max(1);
        let keys: Vec<K> = cache.keys().take(remove).cloned().collect();
        for key in keys {
            cache.remove(&key);
        }
        crate::profile::cache_eviction(category, before, remove, cache.len());
    }
    cache.insert(key, value);
}
