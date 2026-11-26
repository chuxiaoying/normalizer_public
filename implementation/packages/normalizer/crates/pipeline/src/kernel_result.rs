//! State retained at the entry boundary for projection to business PERS.
use super::realtime_state::{EffectSequence, SymbolicStore, TerminalAlternatives};
use super::{fail, Result};
use math::Expr;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) const PERS_SCHEMA: &str = "normalizer.pers/1";

pub(super) struct ExecutionResult {
    pub(super) temporal_definitions: Vec<super::temporal_graph::Definition>,
    pub(super) source_tables: BTreeMap<String, BTreeMap<String, String>>,
    pub(super) entry_return_type: super::Ty,
    pub(super) source_record_return: Option<Vec<(String, Expr)>>,
    pub(super) source_state_bindings: BTreeMap<u64, String>,
    pub(super) source_identity_parameters: BTreeMap<i64, String>,
    pub(super) source_identity_states: BTreeMap<i64, u64>,
    pub(super) realtime_store: SymbolicStore,
    pub(super) realtime_local_allocations: BTreeSet<i64>,
    /// Roots written through a declared host state field and represented in S.
    pub(super) host_state_roots: BTreeSet<i64>,
    pub(super) checked_ports: BTreeMap<String, super::actions::CheckedPort>,
    pub(super) realtime_effects: EffectSequence,
    pub(super) terminal: TerminalAlternatives,
}

impl ExecutionResult {
    pub(super) fn check_visible_heap(&self) -> Result<()> {
        Ok(())
    }

}
