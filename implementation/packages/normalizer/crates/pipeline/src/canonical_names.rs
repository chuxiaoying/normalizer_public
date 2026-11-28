//! Public names known to execution or boundary projection for 统一定形规范.
//! Names order expressions; they do not identify or merge heap objects.
use std::collections::{BTreeMap, BTreeSet};

pub(crate) struct CanonicalNames<'a> {
    pub(crate) heap: &'a BTreeMap<i64, math::HeapBindingIdentity>,
    pub(crate) captured: Option<&'a BTreeMap<String, String>>,
    pub(crate) external: Option<&'a BTreeMap<u64, math::ObservationIdentity>>,
    pub(crate) completion_classes: Option<&'a BTreeMap<u64, Vec<&'a str>>>,
    pub(crate) total_temporals: Option<&'a BTreeSet<u64>>,
    pub(crate) temporals: Option<&'a BTreeMap<u64, usize>>,
}

impl math::SortContext for CanonicalNames<'_> {
    fn captured_parameters(&self) -> Option<&BTreeMap<String, String>> { self.captured }
    fn heap_binding_identity(&self, heap_id: i64) -> Option<&math::HeapBindingIdentity> {
        self.heap.get(&heap_id)
    }

    fn observation_identities(&self) -> Option<&BTreeMap<u64, math::ObservationIdentity>> { self.external }

    fn total_failure_classes(&self, receipt: u64) -> Option<Vec<&str>> {
        self.completion_classes?.get(&receipt).cloned()
    }

    fn temporal_is_total(&self, id: u64) -> bool {
        self.total_temporals.is_some_and(|ids| ids.contains(&id))
    }


    fn temporal_aliases(&self) -> Option<&BTreeMap<u64, usize>> { self.temporals }
}


/// Keys produced for a particular YIELD see only its lexical member slots.
pub(crate) struct ScopedValueKeys<'a> {
    pub(crate) base: CanonicalNames<'a>,
    pub(crate) slots: &'a BTreeMap<u64, usize>,
    pub(crate) cases: &'a std::collections::HashMap<math::Expr, math::CaseArmKeys>,
}

impl math::SortContext for ScopedValueKeys<'_> {
    fn captured_parameters(&self) -> Option<&BTreeMap<String, String>> { self.base.captured_parameters() }
    fn heap_binding_identity(&self, id: i64) -> Option<&math::HeapBindingIdentity> { self.base.heap_binding_identity(id) }
    fn observation_identities(&self) -> Option<&BTreeMap<u64, math::ObservationIdentity>> { self.base.observation_identities() }
    fn temporal_aliases(&self) -> Option<&BTreeMap<u64, usize>> { self.base.temporal_aliases() }
    // Scoped formatting has no completion partition; settled execution
    // supplies those facts directly through CanonicalNames.
    fn case_arm_keys(&self, expression: &math::Expr) -> Option<&math::CaseArmKeys> { self.cases.get(expression) }
    fn has_case_arm_keys(&self) -> bool { !self.cases.is_empty() }
    fn choice_slots(&self) -> Option<&BTreeMap<u64, usize>> { (!self.slots.is_empty()).then_some(self.slots) }
    fn temporal_is_total(&self, id: u64) -> bool { self.base.temporal_is_total(id) }
}
