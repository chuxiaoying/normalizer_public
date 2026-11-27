//! Public names known to execution or boundary projection for 统一定形规范.
//! Names order expressions; they do not identify or merge heap objects.
use std::collections::{BTreeMap, BTreeSet};

pub(crate) struct CanonicalNames<'a> {
    pub(crate) heap: &'a BTreeMap<i64, String>,
    pub(crate) external: Option<&'a BTreeMap<u64, String>>,
    pub(crate) total_temporals: Option<&'a BTreeSet<u64>>,
    pub(crate) reductions: Option<&'a BTreeMap<u64, math::ReduceShape>>,
    pub(crate) temporals: Option<&'a BTreeMap<u64, usize>>,
}

impl math::SortContext for CanonicalNames<'_> {
    fn heap_display_name(&self, heap_id: i64) -> Option<String> {
        self.heap.get(&heap_id).cloned()
    }

    fn external_names(&self) -> Option<&BTreeMap<u64, String>> { self.external }

    fn temporal_is_total(&self, id: u64) -> bool {
        self.total_temporals.is_some_and(|ids| ids.contains(&id))
    }

    fn reduce_shape(&self, id: u64) -> Option<&math::ReduceShape> { self.reductions?.get(&id) }
    fn has_reduce_shapes(&self) -> bool { self.reductions.is_some_and(|shapes| !shapes.is_empty()) }

    fn temporal_aliases(&self) -> Option<&BTreeMap<u64, usize>> { self.temporals }
}


/// Keys produced for a particular YIELD see only its lexical member slots.
pub(crate) struct ChoiceKeys<'a> {
    pub(crate) base: CanonicalNames<'a>,
    pub(crate) slots: &'a BTreeMap<u64, usize>,
    pub(crate) cases: &'a std::collections::HashMap<math::Expr, math::CaseArmKeys>,
}

impl math::SortContext for ChoiceKeys<'_> {
    fn heap_display_name(&self, id: i64) -> Option<String> { self.base.heap_display_name(id) }
    fn external_names(&self) -> Option<&BTreeMap<u64, String>> { self.base.external_names() }
    fn temporal_aliases(&self) -> Option<&BTreeMap<u64, usize>> { self.base.temporal_aliases() }
    fn reduce_shape(&self, id: u64) -> Option<&math::ReduceShape> { self.base.reduce_shape(id) }
    fn has_reduce_shapes(&self) -> bool { self.base.has_reduce_shapes() }
    fn case_arm_keys(&self, expression: &math::Expr) -> Option<&math::CaseArmKeys> { self.cases.get(expression) }
    fn has_case_arm_keys(&self) -> bool { !self.cases.is_empty() }
    fn choice_slots(&self) -> Option<&BTreeMap<u64, usize>> { (!self.slots.is_empty()).then_some(self.slots) }
    fn temporal_is_total(&self, id: u64) -> bool { self.base.temporal_is_total(id) }
}
