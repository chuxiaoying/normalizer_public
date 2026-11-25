//! Bounded native Java control flow runs in the same source frame as its caller.
use super::{
    b, fail, need,
    realtime_state::{Completion, CompletionAlternative, CompletionOutcome},
    Engine, Env, Expr, JavaType, Result, Ty, Value, S,
};
use normalizer_ir::local::ScalarOperand;
use simulator_parser::ast::CatchClause;

impl Engine<'_> {
    /// A lexical child can update an outer binding, but cannot export its own
    /// locals. Returns and effects already live in the same execution frame.
    pub(super) fn source_scope(&mut self, body: &[S], id: &str, env: &mut Env, path: Expr) -> Result<()> {
        let mut locals = env.clone();
        self.source_steps(body, id, &mut locals, path)?;
        for (name, value) in env {
            *value = locals.remove(name).expect("scoped source preserved its outer binding");
        }
        Ok(())
    }

    pub(super) fn source_if(
        &mut self,
        condition: Expr,
        yes: &[S],
        no: &[S],
        id: &str,
        env: &mut Env,
        path: Expr,
    ) -> Result<()> {
        if let Some(truth) = condition.as_bool() {
            return self.source_scope(if truth { yes } else { no }, id, env, path);
        }
        let parent = std::mem::replace(&mut self.realtime_completion, Completion::Continue);
        let original_store = self.realtime_store.clone();
        let effects_start = self.realtime_effects.events().len();
        let mut yes_env = env.clone();
        self.source_steps(
            yes,
            id,
            &mut yes_env,
            math::normalize(b::and(path.clone(), condition.clone())),
        )?;
        let yes_completion = std::mem::replace(&mut self.realtime_completion, Completion::Continue);
        let yes_writes = self.realtime_store.suffix_from(original_store.len());
        self.realtime_store = original_store.clone();
        let mut no_env = env.clone();
        self.source_steps(
            no,
            id,
            &mut no_env,
            math::normalize(b::and(path, b::not(condition.clone()))),
        )?;
        let no_completion = self.realtime_completion.clone();
        let no_writes = self.realtime_store.suffix_from(original_store.len());
        self.realtime_store = original_store;
        self.realtime_store
            .append(super::realtime_state::SymbolicStore::join(
                condition.clone(),
                &yes_writes,
                &no_writes,
            ));
        let yes_live = yes_completion.continuing_guard().as_bool() != Some(false);
        let no_live = no_completion.continuing_guard().as_bool() != Some(false);
        for (name, value) in env {
            let y = yes_env.remove(name).unwrap();
            let n = no_env.remove(name).unwrap();
            *value = match (yes_live, no_live) {
                (true, true) if y != n => super::select(condition.clone(), y, n)?,
                (true, _) => y,
                (_, true) => n,
                _ => continue,
            };
        }
        self.realtime_effects.merge_writes_since(effects_start);
        let joined =
            super::realtime_state::join_completion(condition, yes_completion, no_completion)
                .ok_or_else(|| fail("source branch completion cannot be joined"))?;
        self.realtime_completion = parent.replace_continue(joined);
        Ok(())
    }

    pub(super) fn execute_rectangular_counter(
        &self, rectangle: crate::normalized::fixed_for::RectangularCounter<'_>, env: &mut Env,
    ) -> Result<()> {
        let get = |name: &str| -> Result<Expr> {
            let value = env.get(name).ok_or_else(|| fail(format!("unbound rectangular loop value {name}")))?;
            need(value.ty() == Ty::Integer, "rectangular loop needs exact integer locals")?;
            value.scalar()
        };
        let initial = get(rectangle.target)?;
        let mut domain = b::lit_bool(true);
        let mut delta = b::lit_int(rectangle.increment);
        for (bound, inclusive) in rectangle.bounds {
            let end = get(bound)?;
            let (test, count) = if inclusive {
                (b::ge(end.clone(), b::lit_int(0)), b::add(end, b::lit_int(1)))
            } else {
                (b::gt(end.clone(), b::lit_int(0)), end)
            };
            domain = b::and(domain, test);
            delta = b::mul(delta, count);
        }
        let result = b::case_when(vec![
            (math::normalize(domain), math::normalize(b::add(initial.clone(), delta))),
            (b::lit_bool(true), initial),
        ]);
        env.insert(rectangle.target.into(), Value::Term(Ty::Integer, math::normalize(result)));
        Ok(())
    }

    pub(super) fn execute_index_fold(
        &self, binding: &str, lower: &math::Integer, upper: &ScalarOperand,
        skip_before: &math::Integer, stop_after: &math::Integer, env: &mut Env,
    ) -> Result<()> {
        let current = env.get(binding).ok_or_else(|| fail("indexed fold needs an initialized accumulator"))?;
        need(current.ty() == Ty::Integer, "indexed fold needs a numeric accumulator")?;
        let mut total = current.scalar()?;
        let end = match upper {
            ScalarOperand::Integer(n) => b::lit_integer(n.clone()),
            ScalarOperand::Local(name) => {
                let value = env.get(name).ok_or_else(|| fail("unbound indexed fold upper bound"))?;
                need(value.ty() == Ty::Integer, "indexed fold upper bound must be numeric")?;
                value.scalar()?
            }
            ScalarOperand::Text(_) => return Err(fail("indexed fold upper bound must be numeric")),
        };
        let first = std::cmp::max(lower, skip_before);
        if stop_after < first { return Ok(()); }
        let span: math::Integer = stop_after - first + 1;
        let span: usize = span.try_into().map_err(|_| fail("indexed fold prefix is too large"))?;
        need(span <= 256, "indexed fold prefix is too large; preserve a closed symbolic recurrence")?;
        // The break guard prevents any further committed update. Compute
        // one result for each possible prefix of the remaining indices.
        let mut branches = Vec::with_capacity(span + 1);
        branches.push((b::le(end.clone(), b::lit_integer(first.clone())), total.clone()));
        for n in 0..span {
            let index = first + n;
            total = math::normalize(b::add(total, b::lit_integer(index.clone())));
            let guard = if n + 1 == span { b::lit_bool(true) }
                else { b::eq(end.clone(), b::lit_integer(index + 1)) };
            branches.push((guard, total.clone()));
        }
        env.insert(binding.into(), Value::Term(Ty::Integer, math::normalize(b::case_when(branches))));
        Ok(())
    }

    pub(super) fn count_range(
        &self,
        binding: &str,
        accumulate: bool,
        lower: &ScalarOperand,
        upper: &ScalarOperand,
        env: &mut Env,
    ) -> Result<()> {
        let endpoint = |value: &ScalarOperand| match value {
            ScalarOperand::Integer(n) => Ok(b::lit_integer(n.clone())),
            ScalarOperand::Local(n) => {
                let value = env
                    .get(n)
                    .ok_or_else(|| fail(format!("unbound range endpoint {n}")))?;
                need(value.ty() == Ty::Integer, "integer range endpoint required")?;
                value.scalar()
            }
            ScalarOperand::Text(_) => Err(fail("integer range endpoint required")),
        };
        let length = math::normalize(b::sub(endpoint(upper)?, endpoint(lower)?));
        let count = b::case_when(vec![
            (b::gt(length.clone(), b::lit_int(0)), length),
            (b::lit_bool(true), b::lit_int(0)),
        ]);
        let value = if accumulate {
            let current = env
                .get(binding)
                .ok_or_else(|| fail("undeclared count accumulator"))?;
            need(
                current.ty() == Ty::Integer,
                "integer count accumulator required",
            )?;
            b::add(current.scalar()?, count)
        } else {
            count
        };
        env.insert(
            binding.to_owned(),
            Value::Term(Ty::Integer, math::normalize(value)),
        );
        Ok(())
    }

    pub(super) fn execute_fixed_for(
        &mut self,
        header: crate::normalized::fixed_for::FixedFor<'_>,
        body: &[S],
        id: &str,
        env: &mut Env,
        path: Expr,
    ) -> Result<()> {
        need(!env.contains_key(header.counter), "duplicate loop variable")?;
        let mut locals = env.clone();
        for index in 0..header.count {
            self.budget()?;
            let active = math::normalize(b::and(
                path.clone(),
                self.realtime_completion.continuing_guard(),
            ));
            if active.as_bool() == Some(false) {
                break;
            }
            let counter = b::lit_integer(header.start + index * header.step);
            locals.insert(
                header.counter.into(),
                Value::Term(Ty::Integer, counter.clone()),
            );
            self.source_steps(body, id, &mut locals, active)?;
            need(
                locals[header.counter].scalar()? == counter,
                "loop body changes its proven induction variable",
            )?;
            for (name, value) in env.iter_mut() {
                *value = locals[name].clone();
            }
            locals.retain(|name, _| env.contains_key(name));
        }
        Ok(())
    }

    pub(super) fn execute_source_try(
        &mut self,
        body: &[S],
        catches: &[CatchClause],
        finally: Option<&[S]>,
        id: &str,
        env: &mut Env,
        path: Expr,
    ) -> Result<()> {
        need(
            crate::normalized::protected_return(body) && !catches.is_empty(),
            "source try requires a terminal protected expression",
        )?;
        let parent = std::mem::replace(&mut self.realtime_completion, Completion::Continue);

        let prior_realtime_throws = self.realtime_throws.len();
        self.source_steps(body, id, &mut env.clone(), path.clone())?;
        let mut pending = self.realtime_completion.clone().alternatives();

        self.realtime_throws.truncate(prior_realtime_throws);
        let mut handled = Vec::new();
        for catch in catches {
            let JavaType::Class(caught) = catch.exception_type() else {
                return Err(fail("invalid catch type"));
            };
            let owner = id.rsplit_once('.').map(|(owner, _)| owner).unwrap_or("");
            let caught = crate::normalized::exception_name(
                caught,
                owner,
                &self.project.source_exception_parents,
            )
            .ok_or_else(|| fail(format!("undeclared catch class {caught}")))?;
            let mut guard = b::lit_bool(false);
            pending.retain(|alternative| {
                if let CompletionOutcome::Throw { class, .. } = &alternative.outcome {
                    if crate::normalized::subtype(
                        class,
                        &caught,
                        &self.project.source_exception_parents,
                    ) {
                        guard = b::or(guard.clone(), alternative.guard.clone());
                        return false;
                    }
                }
                true
            });
            let guard = math::normalize(guard);
            if guard.as_bool() == Some(false) {
                continue;
            }
            need(
                catch.body().is_empty() || crate::normalized::protected_return(catch.body()),
                "catch handler must fall through or return its value",
            )?;
            self.realtime_completion = Completion::Continue;
            let mut handler_env = env.clone();
            // The exception object is type-only in this source profile. An
            // observed catch binding requires a declared payload projection.
            handler_env.remove(catch.var_name());
            self.source_steps(
                catch.body(),
                id,
                &mut handler_env,
                math::normalize(b::and(path.clone(), guard.clone())),
            )?;
            for mut outcome in self.realtime_completion.clone().alternatives() {
                outcome.guard = math::normalize(b::and(guard.clone(), outcome.guard));
                handled.push(outcome);
            }
        }
        if let Some(finally) = finally {
            need(catches.iter().all(|handler| crate::normalized::protected_return(handler.body())),
                "source finally requires terminal catch handlers")?;
            pending.extend(handled);
            let saved = Completion::from_alternatives(pending);
            // A pending return or throw is not a closed execution path until
            // finally itself completes. Run the block once over the protected
            // frame's domain. Its abrupt completion overrides the saved one.
            self.realtime_throws.truncate(prior_realtime_throws);
            self.realtime_completion = Completion::Continue;
            self.source_steps(finally, id, &mut env.clone(), path.clone())?;
            let finished = self.realtime_completion.replace_continue(saved);
            self.realtime_throws.truncate(prior_realtime_throws);
            self.realtime_throws.extend(finished.clone().alternatives().into_iter().filter_map(|alt| {
                matches!(alt.outcome, CompletionOutcome::Throw { .. })
                    .then(|| math::normalize(b::and(path.clone(), alt.guard)))
            }));
            self.realtime_completion = parent.replace_continue(finished);
            return Ok(());
        }
        for CompletionAlternative { guard, outcome } in &pending {
            if matches!(outcome, CompletionOutcome::Throw { .. }) {
                let escaped = math::normalize(b::and(path.clone(), guard.clone()));

                self.realtime_throws.push(escaped);
            }
        }
        pending.extend(handled);
        self.realtime_completion = parent.replace_continue(Completion::from_alternatives(pending));
        Ok(())
    }
}
