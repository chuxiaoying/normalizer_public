//! Shared primitives for the real-time symbolic execution state.
//!
//! The first slices cover value joins, the completion channel used by pure
//! return guards, path-local stack/heap fields, and guarded ordered effects.
//! It is used by both source execution and normalized action execution so the
//! earliest branch join has one implementation and one simplification boundary.

use math::{builder as b, Expr};

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
}

impl Completion {
    /// Domain on which the next statement still executes.
    pub(crate) fn continuing_guard(&self) -> Expr {
        match self {
            Self::Continue => b::lit_bool(true),
            Self::Throw { .. } | Self::Return(_) => b::lit_bool(false),
            Self::Guarded(alternatives) => math::normalize(alternatives.iter()
                .filter(|a| a.outcome == CompletionOutcome::Continue)
                .fold(b::lit_bool(false), |guard, a| b::or(guard, a.guard.clone()))),
        }
    }

    /// A total return is certified and collapsed by `from_alternatives`.
    /// The remaining guarded states include a different exit or an uncovered domain.
    pub(crate) fn return_value(&self) -> Option<Expr> {
        match self {
            Self::Return(value) => Some(value.clone()),
            Self::Continue | Self::Throw { .. } | Self::Guarded(_) => None,
        }
    }

    /// Return the terminal alternatives when this completion covers the whole
    /// current domain. A guarded set with a fallthrough path is still an
    /// intermediate block state and cannot be rendered as a function result.
    pub(crate) fn terminal_alternatives(&self) -> Option<Vec<CompletionAlternative>> {
        let alternatives = self.clone().alternatives();
        if alternatives
            .iter()
            .any(|alternative| matches!(alternative.outcome, CompletionOutcome::Continue))
            || coverage(&alternatives).as_bool() != Some(true)
        {
            return None;
        }
        Some(alternatives)
    }

    pub(crate) fn is_always_throwing(&self) -> bool {
        self.terminal_alternatives().is_some_and(|alternatives| {
            !alternatives.is_empty() && alternatives.iter().all(|alternative| {
                matches!(alternative.outcome, CompletionOutcome::Throw { .. })
            })
        })
    }

    /// Project the normal-return part of a terminal completion. Throwing
    /// alternatives leave their domain absent from the CASE expression, which
    /// matches the PERS convention that `R` is defined only on normal return.
    pub(crate) fn normal_return_value(&self) -> Option<Expr> {
        let alternatives = self.terminal_alternatives()?;
        let returns = alternatives
            .into_iter()
            .filter_map(|alternative| match alternative.outcome {
                CompletionOutcome::Return(value) => Some((alternative.guard, value)),
                CompletionOutcome::Continue | CompletionOutcome::Throw { .. } => None,
            })
            .collect::<Vec<_>>();
        if returns.is_empty() {
            return None;
        }
        if returns.iter().all(|(_, value)| value == &returns[0].1) {
            return Some(canonicalize_realtime_case(
                returns.into_iter().next().unwrap().1,
            ));
        }
        let expression = math::normalize(b::normal_return_case_when(returns));
        Some(canonicalize_realtime_case(expression))
    }

    /// R is observed only after normal completion. Once all exits are covered
    /// and throws are published separately, the last normal arm is exhaustive
    /// on that observation domain. Its guard need not retest a failed call.
    pub(crate) fn value_on_normal_exit(&self) -> Option<Expr> {
        let mut returns: Vec<_> = self.terminal_alternatives()?.into_iter().filter_map(|alt| {
            if let CompletionOutcome::Return(value) = alt.outcome {
                Some((alt.guard, value))
            } else {
                None
            }
        }).collect();
        returns.last_mut()?.0 = b::lit_bool(true);
        Some(canonicalize_realtime_case(math::normalize(b::case_when(returns))))
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
        if alternatives.is_empty() {
            return Self::Continue;
        }
        if alternatives.len() == 1 && alternatives[0].guard.as_bool() == Some(true) {
            return outcome_to_completion(alternatives.into_iter().next().unwrap().outcome);
        }
        let covered = coverage(&alternatives).as_bool() == Some(true);
        if covered
            && alternatives
                .iter()
                .all(|alternative| matches!(alternative.outcome, CompletionOutcome::Continue))
        {
            return Self::Continue;
        }
        if covered
            && alternatives
                .iter()
                .all(|alternative| matches!(alternative.outcome, CompletionOutcome::Return(_)))
        {
            return Self::Return(collapse_return_values(&alternatives));
        }
        if covered && alternatives.iter().all(|alternative| alternative.outcome == alternatives[0].outcome) {
            return outcome_to_completion(alternatives[0].outcome.clone());
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
        CompletionOutcome::Throw { class, payload } => Completion::Throw { class, payload },
    }
}

fn coverage(alternatives: &[CompletionAlternative]) -> Expr {
    math::normalize(
        alternatives
            .iter()
            .fold(b::lit_bool(false), |coverage, alternative| {
                b::or(coverage, alternative.guard.clone())
            }),
    )
}

fn collapse_return_values(alternatives: &[CompletionAlternative]) -> Expr {
    let mut reversed = alternatives.iter().rev();
    let CompletionOutcome::Return(mut value) = reversed
        .next()
        .expect("return alternatives are nonempty")
        .outcome
        .clone()
    else {
        unreachable!("return alternatives were checked before collapsing")
    };
    for alternative in reversed {
        let CompletionOutcome::Return(branch) = &alternative.outcome else {
            unreachable!("return alternatives were checked before collapsing");
        };
        value = join_value(alternative.guard.clone(), branch.clone(), value);
    }
    value
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

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum EffectOperation {
    Port(String),
    Sql(super::business_sql::SqlEffect),
    StateWrite(Vec<String>),
    ArrayWrite { root: String, index: i64 },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct EffectEvent {
    pub(crate) guard: Expr,
    pub(crate) sequence: usize,
    pub(crate) operation: EffectOperation,
    pub(crate) callsite: String,
    pub(crate) args: Vec<Expr>,
    pub(crate) result: Option<Expr>,
}

impl EffectEvent {
}

/// Ordered, guarded S events produced at their execution point.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct EffectSequence {
    events: Vec<EffectEvent>,
}

impl EffectSequence {
    pub(crate) fn record(
        &mut self,
        guard: Expr,
        operation: impl Into<String>,
        callsite: impl Into<String>,
        args: Vec<Expr>,
        result: Option<Expr>,
    ) {
        self.events.push(EffectEvent {
            guard: math::normalize(guard),
            sequence: self.events.len(),
            operation: EffectOperation::Port(operation.into()),
            callsite: callsite.into(),
            args,
            result,
        });
    }

    pub(super) fn record_sql(&mut self, guard: Expr, statement: super::business_sql::SqlEffect, args: Vec<Expr>, response: Expr) {
        self.events.push(EffectEvent {
            sequence: self.events.len(),
            guard: math::normalize(guard),
            operation: EffectOperation::Sql(statement),
            callsite: String::new(),
            args,
            result: Some(response),
        });
    }

    pub(super) fn write_state(&mut self, guard: Expr, target: &[String], value: Expr) {
        self.events.push(EffectEvent {
            sequence: self.events.len(),
            operation: EffectOperation::StateWrite(target.to_vec()),
            guard,
            args: vec![value],
            callsite: String::new(),
            result: None,
        });
    }

    pub(super) fn write_array(&mut self, guard: Expr, root: String, index: i64, value: Expr) {
        self.events.push(EffectEvent {
            sequence: self.events.len(),
            operation: EffectOperation::ArrayWrite { root, index },
            guard,
            args: vec![value],
            callsite: String::new(),
            result: None,
        });
    }

    /// Simplify the new effects at a control-flow join. Previously observed
    /// events remain an immutable prefix shared by other live paths.
    pub(super) fn merge_writes_since(&mut self, start: usize) {
        if !self.events[start..].iter().any(|event| matches!(event.operation, EffectOperation::StateWrite(_))) { return; }
        let mut previous_writes = std::collections::BTreeMap::<Vec<String>,usize>::new();
        let suffix = self.events.split_off(start);
        for mut event in suffix {
            if let EffectOperation::StateWrite(target) = &event.operation {
                if let Some(&previous) = previous_writes.get(target) {
                    let prior = &self.events[previous];
                    // Moving this write earlier is valid only when every
                    // intervening event is absent on this write's path.
                    if prior.args == event.args && self.events[previous..].iter().all(|other| {
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
        self.read_with_initial(reference, field, path, None)
    }

    // Entry-state roots have a declared value before any Java statement runs.
    // A branch-local first access must not remove that initial value on another branch.
    pub(crate) fn read_with_initial(
        &self, reference: &Expr, field: &str, path: &Expr, initial: Option<Expr>,
    ) -> Option<Expr> {
        self.read_matching_with_initial(field, path, initial, |candidate| {
            (candidate == reference).then(|| b::lit_bool(true))
        })
    }

    // A write reaches a read only on paths where their source objects
    // denote the same reference. Walk the single store in reverse order.
    pub(crate) fn read_matching_with_initial(
        &self, field: &str, path: &Expr, initial: Option<Expr>,
        mut aliases: impl FnMut(&Expr) -> Option<Expr>,
    ) -> Option<Expr> {
        let mut branches = Vec::new();
        let mut fallback = initial;
        for entry in self.entries.iter().rev().filter(|entry| entry.field == field) {
            let Some(identity) = aliases(&entry.reference) else { continue; };
            let guard = math::normalize(b::and(entry.guard.clone(), identity));
            if guard.as_bool() == Some(false) || b::path_implies(path, &b::not(guard.clone())) {
                continue;
            }
            if guard.as_bool() == Some(true) || b::path_implies(path, &guard) {
                fallback = Some(math::normalize(entry.value.clone()));
                break;
            }
            branches.push((guard, entry.value.clone()));
        }
        if let Some(fallback) = fallback {
            let value = branches
                .into_iter()
                .rev()
                .fold(fallback, |old, (guard, value)| {
                    join_value(guard, value, old)
                });
            return Some(value);
        }
        let coverage = branches
            .iter()
            .fold(b::lit_bool(false), |coverage, (guard, _)| {
                b::or(coverage, guard.clone())
            });
        if !b::path_implies(path, &math::normalize(coverage)) {
            return None;
        }
        let mut branches = branches.into_iter().rev();
        let (_, fallback) = branches.next()?;
        Some(branches.fold(fallback, |old, (guard, value)| {
            join_value(guard, value, old)
        }))
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
    math::normalize(b::case_when(vec![
        (condition, then_value),
        (b::lit_bool(true), else_value),
    ]))
}

fn canonicalize_realtime_case(expression: Expr) -> Expr {
    let Some(mut branches) = expression.as_case_when().map(|branches| branches.to_vec()) else {
        return expression;
    };
    if branches.len() >= 2 {
        let last = branches.len() - 1;
        if branches[last].0.as_bool() == Some(true) {
            let index = last - 1;
            let (condition, yes) = branches[index].clone();
            let no = branches[last].1.clone();
            if let Some(complement) = math::case_orientation::preferred_complement(
                &condition, &yes, &no, &math::DefaultSortContext,
            ) {
                branches[index] = (complement, no);
                branches[last].1 = yes;
            }
        }
    }
    math::normalize(b::normal_return_case_when(branches))
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
