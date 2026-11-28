//! Shared primitives for the real-time symbolic execution state.
//!
//! The first slices cover value joins, the completion channel used by pure
//! return guards, path-local stack/heap fields, and guarded ordered effects.
//! It is used by both source execution and normalized action execution so the
//! earliest branch join has one implementation and one simplification boundary.

use math::{builder as b, Expr};
use super::port_completion::PortSpec;
use std::sync::Arc;

/// The completion channel carried by a real-time execution state.
///
/// A completion is either one outcome or a guarded set of outcomes. The
/// guarded form is needed when a branch has a normal continuation on one path
/// and a return or throw on another path. Keeping those alternatives in the
/// same channel prevents the executor from replacing a mixed state with an
/// invented `Continue`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Completion {
    Continue,
    Return(Expr),
    Throw { class: String, payload: Option<Expr> },
    ThrowValue(Expr),
    Guarded(Vec<CompletionAlternative>),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CompletionAlternative {
    pub(crate) guard: Expr,
    pub(crate) outcome: CompletionOutcome,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum CompletionOutcome {
    Continue,
    Return(Expr),
    Throw { class: String, payload: Option<Expr> },
    ThrowValue(Expr),
}

pub(crate) struct TerminalAlternative {
    pub(crate) guard: Expr,
    pub(crate) outcome: TerminalOutcome,
}

pub(crate) enum TerminalOutcome {
    Return(Expr),
    Throw { class: String, payload: Option<Expr> },
    ThrowValue(Expr),
}

pub(crate) struct TerminalAlternatives(Vec<TerminalAlternative>);

impl TerminalAlternatives {
    pub(crate) fn is_always_throwing(&self) -> bool {
        // The completion proof already rejects an empty domain.
        self.0.iter().all(|a| matches!(a.outcome, TerminalOutcome::Throw { .. } | TerminalOutcome::ThrowValue(_)))
    }

    pub(crate) fn as_slice(&self) -> &[TerminalAlternative] { &self.0 }
    pub(crate) fn total_return_value(&self) -> Option<&Expr> {
        // The only Guarded constructor already collapses this exact form.
        match self.as_slice() {
            [TerminalAlternative { guard, outcome: TerminalOutcome::Return(value) }]
                if guard.as_bool() == Some(true) => Some(value),
            _ => None,
        }
    }

    pub(super) fn normalize_pure_return_choices(&mut self, definitions: &mut Vec<super::temporal_graph::Definition>) {
        if let Some(value) = self.total_return_value()
            .and_then(|value| super::temporal_graph::same_support_choice(value, definitions))
        {
            self.0[0].outcome = TerminalOutcome::Return(value);
        }
    }

    /// Project the normal-return part of a terminal completion. Throwing
    /// alternatives leave their domain absent from the CASE expression, which
    /// matches the PERS convention that `R` is defined only on normal return.
    pub(crate) fn normal_return_value(&self) -> Option<Expr> {
        let mut returns = self.0.iter()
            .filter_map(|alternative| match &alternative.outcome {
                TerminalOutcome::Return(value) => Some((alternative.guard.clone(), value.clone())),
                TerminalOutcome::Throw { .. } | TerminalOutcome::ThrowValue(_) => None,
            })
            .collect::<Vec<_>>();
        let last = returns.pop()?;
        if returns.iter().all(|(_, value)| value == &last.1) {
            let value = returns.into_iter().next().map_or(last.1, |(_, value)| value);
            return Some(canonicalize_realtime_case(value));
        }
        if let [first] = returns.as_slice() {
            if let Some(expression) = native_text_partition(first, &last) {
                return Some(canonicalize_realtime_case(expression));
            }
        }
        Self::strip_suffix_acquired_tags(&mut returns, &last.0);
        let expression = math::normalize(b::normal_return_case_when(returns, last));
        Some(canonicalize_realtime_case(expression))
    }

    /// R is observed only after normal completion. Once all exits are covered
    /// and throws are published separately, the last normal arm is exhaustive
    /// on that observation domain. Its guard need not retest a failed call.
    pub(crate) fn value_on_normal_exit(&self) -> Option<Expr> {
        let mut returns: Vec<_> = self.0.iter().filter_map(|alt| {
            if let TerminalOutcome::Return(value) = &alt.outcome {
                Some((alt.guard.clone(), value.clone()))
            } else {
                None
            }
        }).collect();
        let (last_guard, fallback) = returns.pop()?;
        if let [first] = returns.as_slice() {
            if let Some(expression) = native_text_partition(first, &(last_guard.clone(), fallback.clone())) {
                return Some(canonicalize_realtime_case(expression));
            }
        }
        Self::strip_suffix_acquired_tags(&mut returns, &last_guard);
        Some(canonicalize_realtime_case(math::normalize(b::case_when(returns, fallback))))
    }

    fn strip_suffix_acquired_tags(returns: &mut [(Expr, Expr)], last_guard: &Expr) {
        // On R's normal domain, reaching a later CASE branch excludes all
        // earlier normal outcomes and every throw. An occurrence tag shared
        // by all remaining normal alternatives need not be checked again.
        // Preserve it if even one remaining normal outcome does not prove it.
        let normal_guards: Vec<_> = returns.iter().map(|(guard, _)| guard.clone())
            .chain(std::iter::once(last_guard.clone())).collect();
        for (i, (guard, _)) in returns.iter_mut().enumerate() {
            let mut terms = Vec::new();
            guard.for_each_conjunct(|term| {
                let scoped = (i > 0 && math::domains::completion::throw_tag(term).is_some())
                    || math::domains::source_type_guard::is_positive_text_guard(term);
                let redundant = scoped
                    && normal_guards[i..].iter().all(|alternative| b::path_implies(alternative, term));
                if !redundant { terms.push(term.clone()); }
            });
            *guard = terms.into_iter().fold(b::lit_bool(true), b::and);
        }
    }

}

/// Two different exact text members of one acquired native operand cannot
/// both match. When these are the only normal exits, either member's test
/// selects its original result on the same normal-completion domain.
fn native_text_partition(first: &(Expr, Expr), last: &(Expr, Expr)) -> Option<Expr> {
    fn tests(guard: &Expr) -> Vec<(Expr, Expr, Expr)> {
        let mut result = Vec::new();
        guard.for_each_conjunct(|term| {
            let Some((math::BinOpKind::Eq, a, z)) = term.as_binop() else { return; };
            let unknown = math::domains::source_type_guard::is_native_unknown_parameter;
            let text = |expr: &Expr| expr.as_lit().is_some_and(math::Lit::is_text);
            let (input, literal) = if unknown(a) && text(z) { (a, z) }
                else if unknown(z) && text(a) { (z, a) } else { return; };
            result.push((input.clone(), literal.clone(), term.clone()));
        });
        result
    }
    let last_tests = tests(&last.0);
    let mut best = None;
    for (input, literal, test) in tests(&first.0) {
        for (_, _, opposite) in last_tests.iter().filter(|(other, value, _)| other == &input && value != &literal) {
            for (test, taken, fallback) in [(&test, &first.1, &last.1), (opposite, &last.1, &first.1)] {
                let candidate = b::case_when(vec![(test.clone(), taken.clone())], fallback.clone());
                let key = math::canonical_order_key(&candidate, &math::DefaultSortContext);
                if best.as_ref().is_none_or(|(best_key, _)| key < *best_key) {
                    best = Some((key, candidate));
                }
            }
        }
    }
    best.map(|(_, expression)| expression)
}

impl Completion {
    /// Domain on which the next statement still executes.
    pub(crate) fn continuing_guard(&self) -> Expr {
        match self {
            Self::Continue => b::lit_bool(true),
            Self::Throw { .. } | Self::ThrowValue(_) | Self::Return(_) => b::lit_bool(false),
            Self::Guarded(alternatives) => math::normalize(alternatives.iter()
                .filter(|a| a.outcome == CompletionOutcome::Continue)
                .fold(b::lit_bool(false), |guard, a| b::or(guard, a.guard.clone()))),
        }
    }

    /// Return the terminal alternatives when this completion covers the whole
    /// current domain. A guarded set with a fallthrough path is still an
    /// intermediate block state and cannot be rendered as a function result.
    pub(crate) fn terminal_alternatives(&self) -> Option<TerminalAlternatives> {
        let alternatives = self.clone().alternatives().into_iter().map(|alternative| {
            let outcome = match alternative.outcome {
                CompletionOutcome::Continue => return None,
                CompletionOutcome::Return(value) => TerminalOutcome::Return(value),
                CompletionOutcome::Throw { class, payload } => TerminalOutcome::Throw { class, payload },
                CompletionOutcome::ThrowValue(value) => TerminalOutcome::ThrowValue(value),
            };
            Some(TerminalAlternative { guard: alternative.guard, outcome })
        }).collect::<Option<Vec<_>>>()?;
        covers_terminal_domain(alternatives.iter().map(|alternative| &alternative.guard))
            .then_some(TerminalAlternatives(alternatives))
    }

    pub(crate) fn is_always_throwing(&self) -> bool {
        self.terminal_alternatives().is_some_and(|alternatives| alternatives.is_always_throwing())
    }

    pub(crate) fn normal_return_value(&self) -> Option<Expr> {
        self.terminal_alternatives()?.normal_return_value()
    }

    pub(crate) fn alternatives(self) -> Vec<CompletionAlternative> {
        match self {
            Self::Continue => vec![CompletionAlternative {
                guard: b::lit_bool(true),
                outcome: CompletionOutcome::Continue,
            }],
            Self::Return(value) => vec![CompletionAlternative {
                guard: b::lit_bool(true),
                outcome: CompletionOutcome::Return(value),
            }],
            Self::Throw { class, payload } => vec![CompletionAlternative {
                guard: b::lit_bool(true),
                outcome: CompletionOutcome::Throw { class, payload },
            }],
            Self::ThrowValue(value) => vec![CompletionAlternative {
                guard: b::lit_bool(true),
                outcome: CompletionOutcome::ThrowValue(value),
            }],
            Self::Guarded(alternatives) => alternatives,
        }
    }

    pub(crate) fn from_alternatives(alternatives: Vec<CompletionAlternative>) -> Self {
        let alternatives = alternatives
            .into_iter()
            .filter_map(|mut alternative| {
                alternative.guard = math::normalize(alternative.guard);
                if alternative.guard.as_bool() == Some(false) {
                    return None;
                }
                alternative.outcome = normalize_outcome(alternative.outcome);
                Some(alternative)
            })
            .collect::<Vec<_>>();
        if alternatives.len() == 1 && alternatives[0].guard.as_bool() == Some(true) {
            return outcome_to_completion(alternatives.into_iter().next().unwrap().outcome);
        }
        let covered = covers_terminal_domain(alternatives.iter().map(|alternative| &alternative.guard));
        if covered
            && alternatives
                .iter()
                .all(|alternative| matches!(alternative.outcome, CompletionOutcome::Continue))
        {
            return Self::Continue;
        }
        if covered {
            // Certify and extract in one pass. Non-return alternatives never
            // enter the return-only collapse, even on a covered domain.
            let returns = alternatives.iter().map(|alternative| match &alternative.outcome {
                CompletionOutcome::Return(value) => Some((&alternative.guard, value)),
                _ => None,
            }).collect::<Option<Vec<_>>>();
            if let Some(value) = returns.and_then(|returns| collapse_return_values(returns.into_iter())) {
                return Self::Return(value);
            }
        }
        Self::Guarded(alternatives)
    }

    /// Continue the current execution only on alternatives that still fall
    /// through. Existing returns and throws remain terminal.
    pub(crate) fn replace_continue(&self, replacement: Completion) -> Self {
        let replacement = replacement.alternatives();
        let mut alternatives = Vec::new();
        for current in self.clone().alternatives() {
            if matches!(current.outcome, CompletionOutcome::Continue) {
                for next in &replacement {
                    alternatives.push(CompletionAlternative {
                        guard: math::normalize(b::and(current.guard.clone(), next.guard.clone())),
                        outcome: next.outcome.clone(),
                    });
                }
            } else {
                alternatives.push(current);
            }
        }
        Self::from_alternatives(alternatives)
    }

    /// Resume the caller after a normal return from this frame. Exceptional
    /// alternatives remain terminal and keep their guards. This is the frame
    /// boundary operation that lets a nested call contribute a value without
    /// terminating the caller's own completion channel.
    pub(crate) fn resume_after_normal_return(&self) -> Self {
        let alternatives = self
            .clone()
            .alternatives()
            .into_iter()
            .map(|alternative| CompletionAlternative {
                guard: alternative.guard,
                outcome: match alternative.outcome {
                    CompletionOutcome::Return(_) => CompletionOutcome::Continue,
                    outcome => outcome,
                },
            })
            .collect();
        Self::from_alternatives(alternatives)
    }

    /// Turn a nullable operation into two realtime outcomes. The null domain
    /// terminates with the supplied exception and only the non-null domain may
    /// continue to the next statement.
    pub(crate) fn replace_continue_with_throw(
        &self,
        guard: Expr,
        class: impl Into<String>,
        payload: Option<Expr>,
    ) -> Self {
        let guard = math::normalize(guard);
        let replacement = Self::from_alternatives(vec![
            CompletionAlternative {
                guard: guard.clone(),
                outcome: CompletionOutcome::Throw {
                    class: class.into(),
                    payload,
                },
            },
            CompletionAlternative {
                guard: math::normalize(b::not(guard)),
                outcome: CompletionOutcome::Continue,
            },
        ]);
        self.replace_continue(replacement)
    }
}

fn normalize_outcome(outcome: CompletionOutcome) -> CompletionOutcome {
    match outcome {
        CompletionOutcome::Continue => CompletionOutcome::Continue,
        CompletionOutcome::Return(value) => CompletionOutcome::Return(math::normalize(value)),
        CompletionOutcome::ThrowValue(value) => CompletionOutcome::ThrowValue(value),
        CompletionOutcome::Throw { class, payload } => CompletionOutcome::Throw {
            class,
            payload: payload.map(math::normalize),
        },
    }
}

fn outcome_to_completion(outcome: CompletionOutcome) -> Completion {
    match outcome {
        CompletionOutcome::Continue => Completion::Continue,
        CompletionOutcome::Return(value) => Completion::Return(value),
        CompletionOutcome::ThrowValue(value) => Completion::ThrowValue(value),
        CompletionOutcome::Throw { class, payload } => Completion::Throw { class, payload },
    }
}

fn coverage<'a>(guards: impl Iterator<Item = &'a Expr>) -> Expr {
    math::normalize(guards.cloned().fold(b::lit_bool(false), b::or))
}

fn covers_terminal_domain<'a>(guards: impl Iterator<Item = &'a Expr>) -> bool {
    use math::BinOpKind::{And, Or};

    // A short-circuit Boolean CASE is an acquired predicate's truth
    // partition. Expand it exclusively for this coverage certificate,
    // never for execution, publication, or observation ordering.
    fn proof_boolean(value: &Expr) -> bool {
        value.as_bool().is_some() || matches!(value.as_unary_op(), Some((math::UnaryOpKind::Not, _)))
            || value.as_binop().is_some_and(|(op, _, _)| matches!(op, And | Or)
                || op.swapped_comparison().is_some())
            || matches!(value.as_sym_var(),
                Some(math::SymVarOrigin::Parameter { primitive_type, .. }
                    | math::SymVarOrigin::External { primitive_type, .. }
                    | math::SymVarOrigin::Temporal { primitive_type, .. })
                if primitive_type == "boolean")
    }
    fn proof_guard(value: &Expr, budget: &mut usize) -> Expr {
        if *budget == 0 || !value.has_case_when() {
            return value.clone();
        }
        *budget -= 1;
        if let Some(([(condition, yes)], no)) = value.as_total_case_when() {
            if proof_boolean(yes) && proof_boolean(no) {
                if let Some(bit) = yes.as_bool() {
                    let condition = proof_guard(condition, budget);
                    let no = proof_guard(no, budget);
                    return if bit { b::or(condition, no) } else { b::and(b::not(condition), no) };
                }
                if let Some(bit) = no.as_bool() {
                    let condition = proof_guard(condition, budget);
                    let yes = proof_guard(yes, budget);
                    return if bit { b::or(b::not(condition), yes) } else { b::and(condition, yes) };
                }
            }
        }
        if let Some((math::UnaryOpKind::Not, operand)) = value.as_unary_op() {
            return b::not(proof_guard(operand, budget));
        }
        if let Some((op @ (And | Or), left, right)) = value.as_binop() {
            return b::binop_public(op, proof_guard(left, budget), proof_guard(right, budget));
        }
        value.clone()
    }

    let guards = guards.collect::<Vec<_>>();
    if coverage(guards.iter().copied()).as_bool() == Some(true) {
        return true;
    }
    if !guards.iter().any(|guard| guard.has_case_when())
        || guards.iter().any(|guard| guard.may_observe_sql_unknown())
    {
        return false;
    }
    let mut budget = 32;
    let proof = guards.iter().map(|guard| proof_guard(guard, &mut budget)).collect::<Vec<_>>();
    proof.iter().zip(guards).any(|(candidate, original)| candidate != original)
        && coverage(proof.iter()).as_bool() == Some(true)
}

fn collapse_return_values<'a>(
    mut returns: impl DoubleEndedIterator<Item = (&'a Expr, &'a Expr)>,
) -> Option<Expr> {
    let (last_guard, last_value) = returns.next_back()?;
    // Coverage was proved for the whole ordered set. Normalize one CASE
    // in that scope, not a right-fold of artificial binary domains whose
    // fallback would also run on a previously matched path.
    let branches: Vec<_> = returns.map(|(guard, value)| (guard.clone(), value.clone())).collect();
    // With exactly two covered outcomes the first guard and its fallback
    // really are a complete binary choice, so CF-04 applies directly.
    if let [(condition, value)] = branches.as_slice() {
        return Some(join_value(condition.clone(), value.clone(), last_value.clone()));
    }
    Some(math::normalize(b::normal_return_case_when(
        branches, (last_guard.clone(), last_value.clone()),
    )))
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct StoreEntry {
    pub(crate) guard: Expr,
    pub(crate) reference: Expr,
    pub(crate) field: String,
    pub(crate) value: Expr,
}

/// Path-local symbolic storage for stack slots and heap fields.
///
/// A branch owns a cloned store while it executes. `join` then prefixes each
/// branch entry with its path guard. Reads fold the latest guarded assignment
/// first and keep an uncovered read unresolved instead of inventing a default.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct SymbolicStore {
    entries: Vec<StoreEntry>,
}

/// A fresh external value and the identity used to publish that occurrence.
/// Keep arbitrary symbolic expressions out of effect receipt slots.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct EffectReceipt {
    binding_id: u64,
    response: Expr,
}

impl EffectReceipt {
    pub(super) fn tagged(binding_id: u64) -> Self {
        Self { binding_id, response: b::external_completion(binding_id) }
    }

    pub(super) fn binding_id(&self) -> u64 { self.binding_id }
    pub(super) fn response(&self) -> &Expr { &self.response }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Acquisition {
    Clock,
    Sql { source: String, after: Vec<(u64, Expr)> },
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum FieldWriteTarget {
    EntryState(Vec<String>),
    Parameter(String),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum EffectOperation {
    Acquire { acquisition: Acquisition, binding_id: u64 },
    Port {
        id: String,
        receipt: EffectReceipt,
        completion: Arc<PortSpec>,
        source_identity: Option<String>,
    },
    Sql { statement: super::business_sql::SqlEffect, receipt: EffectReceipt },
    StateWrite { target: FieldWriteTarget, value: Expr },
    ArrayWrite { root: String, index: i64, value: Expr },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct EffectEvent {
    pub(crate) guard: Expr,
    pub(crate) sequence: usize,
    pub(crate) operation: EffectOperation,
    pub(crate) callsite: String,
    args: Vec<Expr>,
}

impl EffectEvent {
    pub(crate) fn arguments(&self) -> &[Expr] {
        match &self.operation {
            EffectOperation::StateWrite { value, .. } | EffectOperation::ArrayWrite { value, .. } =>
                std::slice::from_ref(value),
            EffectOperation::Port { .. } | EffectOperation::Sql { .. } | EffectOperation::Acquire { .. } => &self.args,
        }
    }
}

/// Ordered, guarded observations and S events at their execution point.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct EffectSequence {
    events: Vec<EffectEvent>,
}

impl EffectSequence {
    fn push_live(&mut self, mut event: EffectEvent) {
        event.guard = math::normalize(event.guard);
        if event.guard.as_bool() != Some(false) {
            self.events.push(event);
        }
    }

    pub(super) fn acquire(&mut self, guard: Expr, acquisition: Acquisition, binding_id: u64) {
        self.push_live(EffectEvent {
            sequence: self.events.len(), guard,
            operation: EffectOperation::Acquire { acquisition, binding_id },
            callsite: String::new(), args: vec![],
        });
    }

    pub(super) fn record_port(
        &mut self,
        guard: Expr,
        operation: impl Into<String>,
        callsite: impl Into<String>,
        args: Vec<Expr>,
        receipt: EffectReceipt,
        completion: Arc<PortSpec>,
    ) {
        self.record_port_occurrence(guard, operation.into(), callsite.into(), args, receipt, completion, None);
    }

    pub(super) fn record_source_port(
        &mut self, guard: Expr, id: &str, args: Vec<Expr>, receipt: EffectReceipt,
        completion: Arc<PortSpec>, source_identity: Option<String>,
    ) {
        self.record_port_occurrence(guard, id.into(), id.into(), args, receipt, completion, source_identity);
    }

    fn record_port_occurrence(
        &mut self, guard: Expr, id: String, callsite: String, args: Vec<Expr>,
        receipt: EffectReceipt, completion: Arc<PortSpec>, source_identity: Option<String>,
    ) {
        self.push_live(EffectEvent {
            guard,
            sequence: self.events.len(),
            operation: EffectOperation::Port { id, receipt, completion, source_identity },
            callsite,
            args,
        });
    }

    pub(super) fn record_sql(&mut self, guard: Expr, statement: super::business_sql::SqlEffect, args: Vec<Expr>, receipt: EffectReceipt) {
        self.push_live(EffectEvent {
            sequence: self.events.len(),
            guard,
            operation: EffectOperation::Sql { statement, receipt },
            callsite: String::new(),
            args,
        });
    }

    pub(super) fn write_state(&mut self, guard: Expr, target: &[String], value: Expr) {
        self.write_field(guard, FieldWriteTarget::EntryState(target.to_vec()), value);
    }

    pub(super) fn write_parameter_field(&mut self, guard: Expr, target: String, value: Expr) {
        self.write_field(guard, FieldWriteTarget::Parameter(target), value);
    }

    fn write_field(&mut self, guard: Expr, target: FieldWriteTarget, value: Expr) {
        self.push_live(EffectEvent {
            sequence: self.events.len(),
            operation: EffectOperation::StateWrite { target, value },
            guard,
            args: Vec::new(),
            callsite: String::new(),
        });
    }

    pub(super) fn write_array(&mut self, guard: Expr, root: String, index: i64, value: Expr) {
        self.push_live(EffectEvent {
            sequence: self.events.len(),
            operation: EffectOperation::ArrayWrite { root, index, value },
            guard,
            args: Vec::new(),
            callsite: String::new(),
        });
    }

    /// Simplify the new effects at a control-flow join. Previously observed
    /// events remain an immutable prefix shared by other live paths.
    pub(super) fn merge_writes_since(&mut self, start: usize) {
        if !self.events[start..].iter().any(|event| matches!(event.operation, EffectOperation::StateWrite { .. })) { return; }
        let mut previous_writes = std::collections::BTreeMap::<FieldWriteTarget,usize>::new();
        let suffix = self.events.split_off(start);
        for mut event in suffix {
            if let EffectOperation::StateWrite { target, .. } = &event.operation {
                if let Some(&previous) = previous_writes.get(target) {
                    let prior = &self.events[previous];
                    // Moving this write earlier is valid only when every
                    // intervening event is absent on this write's path.
                    if prior.arguments() == event.arguments() && self.events[previous..].iter().all(|other| {
                        math::normalize(b::and(other.guard.clone(),event.guard.clone())).as_bool() == Some(false)
                    }) {
                        self.events[previous].guard = math::normalize(b::or(prior.guard.clone(),event.guard));
                        continue;
                    }
                }
                previous_writes.insert(target.clone(),self.events.len());
            } else {
                // Calls can observe the state at their original position.
                previous_writes.clear();
            }
            event.sequence = self.events.len();
            self.events.push(event);
        }
    }

    /// Slots count acquired observations of either kind, not writes that
    /// may be coalesced at joins. Extending the ledger preserves its prefix.
    pub(super) fn observation_identities(&self) -> impl Iterator<Item = (u64, math::ObservationIdentity)> + '_ {
        self.events.iter().filter_map(|event| match &event.operation {
            EffectOperation::Acquire { binding_id, .. } => Some((*binding_id, false)),
            EffectOperation::Port { receipt, .. } | EffectOperation::Sql { receipt, .. } =>
                Some((receipt.binding_id(), true)),
            EffectOperation::StateWrite { .. } | EffectOperation::ArrayWrite { .. } => None,
        }).enumerate().map(|(slot, (id, receipt))| {
            let identity = if receipt { math::ObservationIdentity::Receipt(slot) }
                else { math::ObservationIdentity::RuntimeObservation(slot) };
            (id, identity)
        })
    }

    pub(crate) fn total_failure_contracts(&self) -> impl Iterator<Item = (u64, Vec<&str>)> + '_ {
        self.events.iter().filter_map(|event| {
            if event.guard.as_bool() != Some(true) { return None; }
            match &event.operation {
                EffectOperation::Port { receipt, completion, .. } if !completion.exceptions.is_empty() =>
                    Some((receipt.binding_id(), completion.exceptions.keys().map(String::as_str).collect())),
                EffectOperation::Sql { statement, receipt } => Some((receipt.binding_id(), vec![statement.exception()])),
                _ => None,
            }
        })
    }

    pub(crate) fn total_failure_classes(&self, binding: u64) -> Option<Vec<&str>> {
        self.total_failure_contracts().find_map(|(id, class)| (id == binding).then_some(class))
    }

    pub(crate) fn events(&self) -> &[EffectEvent] {
        &self.events
    }
}

impl SymbolicStore {

    pub(crate) fn store_guarded(
        &mut self,
        guard: Expr,
        reference: Expr,
        field: impl Into<String>,
        value: Expr,
    ) {
        self.entries.push(StoreEntry {
            guard: math::normalize(guard),
            reference,
            field: field.into(),
            value,
        });
    }


    pub(crate) fn read_on_path(&self, reference: &Expr, field: &str, path: &Expr) -> Option<Expr> {
        let (mut branches, fallback) = self.reaching_writes(field, path, |candidate| {
            (candidate == reference).then(|| b::lit_bool(true))
        });
        let fallback = match fallback {
            Some(value) => value,
            None => {
                // Only a complete set of writes can supply a value if the
                // object has no initial field on this path.
                let oldest = branches.last()?.1.clone();
                let covered = coverage(branches.iter().map(|(guard, _)| guard));
                if !b::path_implies(path, &covered) { return None; }
                branches.pop();
                oldest
            }
        };
        Some(Self::apply_writes(branches, fallback))
    }

    // A declared initial value exists even when no write reaches the read.
    pub(crate) fn read_with_initial(
        &self, reference: &Expr, field: &str, path: &Expr, initial: Expr,
    ) -> Expr {
        self.read_matching_with_initial(field, path, initial, |candidate| {
            (candidate == reference).then(|| b::lit_bool(true))
        })
    }

    pub(crate) fn read_matching_with_initial(
        &self, field: &str, path: &Expr, initial: Expr,
        aliases: impl FnMut(&Expr) -> Option<Expr>,
    ) -> Expr {
        let (branches, fallback) = self.reaching_writes(field, path, aliases);
        Self::apply_writes(branches, fallback.unwrap_or(initial))
    }

    // A write reaches a read only when the same field, source identity and
    // path all match. Newest writes take priority over earlier ones.
    fn reaching_writes(
        &self, field: &str, path: &Expr, mut aliases: impl FnMut(&Expr) -> Option<Expr>,
    ) -> (Vec<(Expr, Expr)>, Option<Expr>) {
        let mut branches = Vec::new();
        for entry in self.entries.iter().rev().filter(|entry| entry.field == field) {
            let Some(identity) = aliases(&entry.reference) else { continue; };
            let guard = math::normalize(b::and(entry.guard.clone(), identity));
            if guard.as_bool() == Some(false) || b::path_implies(path, &b::not(guard.clone())) {
                continue;
            }
            if guard.as_bool() == Some(true) || b::path_implies(path, &guard) {
                return (branches, Some(math::normalize(entry.value.clone())));
            }
            branches.push((guard, entry.value.clone()));
        }
        (branches, None)
    }

    fn apply_writes(branches: Vec<(Expr, Expr)>, initial: Expr) -> Expr {
        branches.into_iter().rev().fold(initial, |old, (guard, value)| {
            join_value(guard, value, old)
        })
    }

    pub(crate) fn suffix_from(&self, start: usize) -> Self {
        Self {
            entries: self.entries.iter().skip(start).cloned().collect(),
        }
    }

    pub(crate) fn append(&mut self, suffix: Self) {
        self.entries.extend(suffix.entries);
    }

    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }


    pub(crate) fn entries(&self) -> &[StoreEntry] {
        &self.entries
    }
}

/// Join two values at the first common continuation of a branch.
///
/// The result is normalized immediately.  Callers pass expressions whose
/// local bindings have already been substituted, so a source local name can
/// never become an atom in the merged value.
pub(crate) fn join_value(condition: Expr, then_value: Expr, else_value: Expr) -> Expr {
    math::normalize(b::case_when(vec![(condition, then_value)], else_value))
}

fn canonicalize_realtime_case(expression: Expr) -> Expr {
    let Some((last, earlier)) = expression.as_case_when().and_then(|arms| arms.split_last()) else {
        return expression;
    };
    // Finish the whole CASE under the shared rules. The last two arms of
    // a multi-arm chain are not a free-standing binary decision: the
    // earlier arms delimit their remaining domain and priority.
    let expression = math::normalize(b::normal_return_case_when(earlier.to_vec(), last.clone()));
    factor_array_sum_delta(&expression).unwrap_or(expression)
}

// A successful slot replacement and its caught-failure return both observe
// the same input total. Reuse that acquired pure sum; do not move the
// possibly out-of-bounds slot observation outside the success arm.
fn factor_array_sum_delta(expression: &Expr) -> Option<Expr> {
    use math::domains::relation::RelationExpr;
    use math::{BinOpKind::{Add, Sub}, RelationAggregateKind::{FirstValue, NthValue, Sum}};
    let ([(guard, yes)], no) = expression.as_total_case_when()? else { return None; };
    let ctx = &math::DefaultSortContext;
    let mut best = None;
    for (adjusted, original, enabled) in [(yes, no, guard.clone()), (no, yes, b::not(guard.clone()))] {
        let Some((Sum, RelationExpr::Base { name, alias, snapshot: None }, Some(row), None)) = original.as_relation_aggregate() else { continue; };
        if alias != "value" || row.as_row_value() != Some("value") { continue; }
        let Some((Add, left, right)) = adjusted.as_binop() else { continue; };
        for (remainder, replacement) in [(left, right), (right, left)] {
            let Some((Sub, total, slot)) = remainder.as_binop() else { continue; };
            if total != original { continue; }
            let Some((FirstValue | NthValue, RelationExpr::Base { name: source, alias, snapshot: None }, _, _)) =
                slot.as_relation_aggregate() else { continue; };
            if source != name || alias != "value" { continue; }
            let delta = b::case_when(vec![(enabled.clone(), b::sub(replacement.clone(), slot.clone()))], b::lit_int(0));
            let candidate = math::normalize(b::add(original.clone(), delta));
            if math::canonical_order::canonical_cost_compare(&candidate, expression, ctx).is_lt()
                && best.as_ref().is_none_or(|old| math::canonical_order_compare(&candidate, old, ctx).is_lt())
            {
                best = Some(candidate);
            }
        }
    }
    best
}

/// Join two completion outcomes at their earliest common continuation.
///
/// Outcomes with the same completion kind can be merged symbolically. Mixed
/// terminal kinds stay in one guarded completion instead of being discarded.
/// The guard is the branch condition that led to the outcome, so later catch
/// dispatch can consume only the alternatives it accepts.
pub(crate) fn join_completion(
    condition: Expr,
    then_completion: Completion,
    else_completion: Completion,
) -> Option<Completion> {
    match (then_completion, else_completion) {
        (Completion::Continue, Completion::Continue) => Some(Completion::Continue),
        (Completion::Return(then_value), Completion::Return(else_value)) => Some(
            Completion::Return(join_value(condition, then_value, else_value)),
        ),
        (then_completion, else_completion) => {
            let condition = math::normalize(condition);
            let inverse = math::normalize(b::not(condition.clone()));
            let alternatives = then_completion
                .alternatives()
                .into_iter()
                .map(|alternative| CompletionAlternative {
                    guard: math::normalize(b::and(condition.clone(), alternative.guard)),
                    outcome: alternative.outcome,
                })
                .chain(
                    else_completion
                        .alternatives()
                        .into_iter()
                        .map(|alternative| CompletionAlternative {
                            guard: math::normalize(b::and(inverse.clone(), alternative.guard)),
                            outcome: alternative.outcome,
                        }),
                )
                .collect();
            Some(Completion::from_alternatives(alternatives))
        }
    }
}
