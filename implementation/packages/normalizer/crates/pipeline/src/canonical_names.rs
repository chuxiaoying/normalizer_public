//! Public names known to execution or boundary projection for 统一正则规范.
//! Names order expressions; they do not identify or merge heap objects.
use std::collections::BTreeMap;

pub(crate) struct CanonicalNames<'a> {
    pub(crate) heap: &'a BTreeMap<i64, String>,
    pub(crate) external: Option<&'a BTreeMap<u64, String>>,
    pub(crate) folds: Option<&'a BTreeMap<u64, usize>>,
}

impl math::SortContext for CanonicalNames<'_> {
    fn heap_display_name(&self, heap_id: i64) -> Option<String> {
        self.heap.get(&heap_id).cloned()
    }

    fn external_names(&self) -> Option<&BTreeMap<u64, String>> { self.external }

    fn temporal_aliases(&self) -> Option<&BTreeMap<u64, usize>> { self.folds }
}
