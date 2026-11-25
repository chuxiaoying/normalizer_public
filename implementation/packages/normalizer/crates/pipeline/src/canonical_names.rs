//! Public names known to execution or boundary projection for 统一正则规范.
//! Names order expressions; they do not identify or merge heap objects.
use std::collections::BTreeMap;

pub(crate) struct CanonicalHeapNames<'a>(pub(crate) &'a BTreeMap<i64, String>);

impl math::SortContext for CanonicalHeapNames<'_> {
    fn heap_display_name(&self, heap_id: i64) -> Option<String> {
        self.0.get(&heap_id).cloned()
    }
}
