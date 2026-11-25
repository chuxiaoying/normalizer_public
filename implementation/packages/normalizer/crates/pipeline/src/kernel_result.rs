//! State retained at the entry boundary for projection to business PERS.
use super::realtime_state::{Completion, EffectSequence, SymbolicStore};
use super::{fail, Result};
use math::Expr;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) const PERS_SCHEMA: &str = "normalizer.pers/1";

pub(super) struct ExecutionResult {
    pub(super) temporal_definitions: Vec<super::temporal_graph::Definition>,
    pub(super) source_tables: BTreeMap<String, BTreeMap<String, String>>,
    pub(super) source_record_return: Option<Vec<(String, Expr)>>,
    pub(super) source_state_bindings: BTreeMap<u64, String>,
    pub(super) source_identity_parameters: BTreeMap<i64, String>,
    pub(super) realtime_store: SymbolicStore,
    pub(super) realtime_local_allocations: BTreeSet<i64>,
    /// Roots written through a declared host state field and represented in S.
    pub(super) host_state_roots: BTreeSet<i64>,
    pub(super) realtime_effects: EffectSequence,
    pub(super) realtime_completion: Completion,
}

impl ExecutionResult {
    pub(super) fn check_visible_heap(&self) -> Result<()> {
        if self.realtime_store.entries().iter().any(|entry| {
            entry.guard.as_bool() != Some(false)
                && entry.reference.as_heap_ref().is_none_or(|id| {
                    !self.realtime_local_allocations.contains(&id)
                        && !self.host_state_roots.contains(&id)
                })
        }) {
            return Err(fail(
                "externally visible heap write requires a business field projection",
            ));
        }
        Ok(())
    }

    pub(super) fn business_return(&self) -> Result<Expr> {
        self.check_visible_heap()?;
        if !self.realtime_effects.events().is_empty() {
            return Err(fail("entry has observations outside a scalar return"));
        }
        self.realtime_completion
            .return_value()
            .ok_or_else(|| fail("missing total normal return"))
    }
}
