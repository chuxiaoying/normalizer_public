//! Bounded native Java control flow runs in the same source frame as its caller.
use super::{
    b, fail, need,
    realtime_state::{Completion, CompletionAlternative, CompletionOutcome},
    Engine, Env, Expr, Result, Ty, Value, S,
};
use normalizer_ir::local::IntegerOperand;
use simulator_parser::ast::CatchClause;
use simulator_parser::JavaType;

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

    pub(super) fn source_switch(
        &mut self, selector: Expr,
        cases: &[(simulator_parser::ast::JavaExpr, Vec<S>)],
        default: &[S], id: &str, env: &mut Env, path: Expr,
    ) -> Result<()> {
        let Some(((label, body), rest)) = cases.split_first() else {
            return self.source_scope(default, id, env, path);
        };
        use simulator_parser::ast::{JavaExpr, JavaLit};
        let label = match label {
            JavaExpr::Lit(JavaLit::Int(value)) => i64::from(*value),
            JavaExpr::Lit(JavaLit::Char(value)) => i64::from(*value),
            _ => return Err(fail("switch label requires an int or char literal")),
        };
        let condition = self.condition_under_path(b::eq(selector.clone(), b::lit_int(label)), &path);
        self.source_branch(condition, body, id, env, path,
            |engine, locals, path| engine.source_switch(selector, rest, default, id, locals, path))
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
        self.source_branch(condition, yes, id, env, path,
            |engine, locals, path| engine.source_scope(no, id, locals, path))
    }

    fn source_branch(
        &mut self, condition: Expr, yes: &[S], id: &str, env: &mut Env, path: Expr,
        otherwise: impl FnOnce(&mut Self, &mut Env, Expr) -> Result<()>,
    ) -> Result<()> {
        if let Some(truth) = condition.as_bool() {
            return if truth { self.source_scope(yes, id, env, path) }
                else { otherwise(self, env, path) };
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
        otherwise(self, &mut no_env, math::normalize(b::and(path, b::not(condition.clone()))))?;
        let no_completion = self.realtime_completion.clone();
        let no_writes = self.realtime_store.suffix_from(original_store.len());
        self.realtime_store = original_store;
        // Each branch ran with its full path. Its writes already carry that
        // path (and any additional normal-completion or bounds guard), just
        // like the effects recorded at the write. Keep those guards as-is.
        self.realtime_store.append(yes_writes);
        self.realtime_store.append(no_writes);
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
        let result = b::case_when(
            vec![(math::normalize(domain), math::normalize(b::add(initial.clone(), delta)))],
            initial,
        );
        env.insert(rectangle.target.into(), Value::Term(Ty::Integer, math::normalize(result)));
        Ok(())
    }

    pub(super) fn execute_index_fold(
        &self, binding: &str, lower: &math::Integer, upper: &str,
        skip_before: &math::Integer, stop_after: &math::Integer, env: &mut Env,
    ) -> Result<()> {
        let current = env.get(binding).ok_or_else(|| fail("indexed fold needs an initialized accumulator"))?;
        need(current.ty() == Ty::Integer, "indexed fold needs a numeric accumulator")?;
        let mut total = current.scalar()?;
        let bound = env.get(upper).ok_or_else(|| fail("unbound indexed fold upper bound"))?;
        need(bound.ty() == Ty::Integer, "indexed fold upper bound must be numeric")?;
        let end = bound.scalar()?;
        let first = std::cmp::max(lower, skip_before);
        if stop_after < first { return Ok(()); }
        let span: math::Integer = stop_after - first + 1;
        let budget_error = || fail("[NORM-ANALYSIS-INCOMPLETE] indexed fold prefix exceeds its analysis budget");
        let span: usize = span.try_into().map_err(|_| budget_error())?;
        if span > 256 { return Err(budget_error()); }
        // The break guard prevents any further committed update. Compute
        // one result for each possible prefix of the remaining indices.
        let mut branches = Vec::with_capacity(span + 1);
        branches.push((b::le(end.clone(), b::lit_integer(first.clone())), total.clone()));
        for n in 0..span {
            let index = first + n;
            total = math::normalize(b::add(total, b::lit_integer(index.clone())));
            if n + 1 < span {
                branches.push((b::eq(end.clone(), b::lit_integer(index + 1)), total.clone()));
            }
        }
        env.insert(binding.into(), Value::Term(Ty::Integer, math::normalize(b::case_when(branches, total))));
        Ok(())
    }

    pub(super) fn count_range(
        &self,
        binding: &str,
        accumulate: bool,
        lower: &IntegerOperand,
        upper: &IntegerOperand,
        env: &mut Env,
    ) -> Result<()> {
        let endpoint = |value: &IntegerOperand| match value {
            IntegerOperand::Integer(n) => Ok(b::lit_integer(n.clone())),
            IntegerOperand::Local(n) => {
                let value = env
                    .get(n)
                    .ok_or_else(|| fail(format!("unbound range endpoint {n}")))?;
                need(value.ty() == Ty::Integer, "integer range endpoint required")?;
                value.scalar()
            }
        };
        let length = math::normalize(b::sub(endpoint(upper)?, endpoint(lower)?));
        let count = b::case_when(
            vec![(b::gt(length.clone(), b::lit_int(0)), length)],
            b::lit_int(0),
        );
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

    pub(super) fn execute_fixed_field_for(
        &mut self, header: crate::normalized::fixed_for::FixedFieldFor<'_>,
        body: &[S], id: &str, env: &mut Env, path: Expr,
    ) -> Result<()> {
        let unproved = || crate::preprocess::required(
            "NORM-LOOP", id, "field induction needs one acquired private allocation for initialization, test and update",
            "确认循环头的每个接收者都指向同一个本地对象。",
        );
        let get = |name: &str| env.get(name).and_then(|value| value.scalar().ok()).ok_or_else(unproved);
        let [init, test, update] = header.receivers;
        let reference = get(init)?;
        if get(test)? != reference || get(update)? != reference {
            return Err(unproved().into());
        }
        let owner = self.source_objects.allocation_owner(&reference).ok_or_else(unproved)?;
        let field = self.program.find_class(owner).and_then(|class| class.fields().iter().find(|f| f.name() == header.field))
            .ok_or_else(unproved)?;
        let final_value = header.start + header.count * header.step;
        let stores_fit = |value| match field.java_type() {
            JavaType::Primitive(name) if name == "int" => i32::try_from(value).is_ok(),
            JavaType::Primitive(name) if name == "long" => i64::try_from(value).is_ok(),
            _ => false,
        };
        if field.is_static() || field.is_final() || !stores_fit(header.start) || !stores_fit(final_value) {
            return Err(unproved().into());
        }
        let value = |counter| Value::Term(Ty::Integer, b::lit_integer(counter));
        // The header store happens even for a zero-trip loop.
        self.write_source_field(&reference, header.field, value(header.start), path.clone())?;
        for offset in 0..header.count {
            self.budget()?;
            self.source_steps(body, id, env, path.clone())?;
            let next = header.start + (offset + 1) * header.step;
            self.write_source_field(&reference, header.field, value(next), path.clone())?;
        }
        Ok(())
    }

    pub(super) fn execute_bounded_control_for(
        &mut self, proof: crate::normalized::fixed_for::BoundedControlFor<'_>,
        id: &str, env: &mut Env, path: Expr,
    ) -> Result<()> {
        let bound = proof.bound.map(|name| {
            let value = env.get(name).ok_or_else(|| fail("unbound finite loop limit"))?;
            need(value.ty() == Ty::Integer, "finite loop limit is not an exact integer")?;
            value.scalar()
        }).transpose()?;
        let mut locals = env.clone();
        let mut exits: Vec<(i128, Env)> = Vec::new();
        for (index, statements) in proof.iterations {
            self.budget()?;
            let mut active = path.clone();
            if let Some(limit) = &bound {
                let cut = index - i128::from(proof.inclusive);
                let enter = b::gt(limit.clone(), b::lit_integer(cut));
                let enter = self.condition_under_path(enter, &path);
                match enter.as_bool() {
                    Some(false) => break,
                    Some(true) => {}
                    None => {
                        exits.push((cut, locals.clone()));
                        active = math::normalize(b::and(path.clone(), enter));
                    }
                }
            }
            locals.insert(proof.counter.into(), Value::Term(Ty::Integer, b::lit_integer(index)));
            for statement in statements {
                self.source_steps(std::slice::from_ref(statement), id, &mut locals, active.clone())?;
            }
        }
        for (name, value) in env.iter_mut() {
            let finish = &locals[name];
            let Some(bound) = &bound else {
                *value = finish.clone();
                continue;
            };
            let mut cuts: Vec<(i128, &Value)> = Vec::new();
            for (cut, exit) in &exits {
                let observed = &exit[name];
                if let Some((end, previous)) = cuts.last_mut() {
                    if *previous == observed {
                        *end = *cut;
                        continue;
                    }
                }
                cuts.push((*cut, observed));
            }
            if cuts.last().is_some_and(|(_, observed)| *observed == finish) {
                cuts.pop();
            }
            if cuts.is_empty() {
                *value = finish.clone();
                continue;
            }
            // Prefix-bound domains are consecutive ordered integers. Omit
            // already excluded lower bounds and represent a one-point
            // middle interval as an equality, not an expanded conjunction.
            need(finish.ty() == Ty::Integer, "native closed loop only changes exact integer local values")?;
            let mut previous = None;
            let mut arms = Vec::with_capacity(cuts.len());
            for (end, observed) in cuts {
                let point = b::lit_integer(end);
                let guard = if previous.is_some_and(|start| start + 1 == end) {
                    b::eq(bound.clone(), point)
                } else {
                    b::le(bound.clone(), point)
                };
                arms.push((guard, observed.scalar()?));
                previous = Some(end);
            }
            *value = Value::Term(Ty::Integer, math::normalize(b::case_when(arms, finish.scalar()?)));
        }
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
        let counter_value = |value| Value::Term(Ty::Integer, b::lit_integer(value));
        if header.assigns_outer {
            *env.get_mut(header.counter).expect("verified initialized loop counter") =
                counter_value(header.start);
        } else {
            need(!env.contains_key(header.counter), "duplicate loop variable")?;
        }
        let mut locals = env.clone();
        let mut offset = 0;
        let not_proven = || crate::preprocess::required(
            "NORM-LOOP", id, "loop body does not prove strictly forward induction",
            "保留源码；需要证明所有继续执行的路径都朝循环终点前进。",
        );
        while offset < header.count {
            let active = math::normalize(b::and(
                path.clone(),
                self.realtime_completion.continuing_guard(),
            ));
            if active.as_bool() == Some(false) {
                break;
            }
            self.budget()?;
            let current = header.start + offset * header.step;
            let counter = b::lit_integer(current);
            locals.insert(
                header.counter.into(),
                Value::Term(Ty::Integer, counter.clone()),
            );
            self.source_steps(body, id, &mut locals, active)?;
            if self.realtime_completion.continuing_guard().as_bool() == Some(false) { break; }
            let observed: i128 = locals[header.counter].scalar()?.as_lit().and_then(|l| l.as_integer())
                .and_then(|n| n.try_into().ok()).ok_or_else(not_proven)?;
            // Do not forecast the number of remaining steps using stale
            // values. A forward body write may skip iterations, while a
            // backward write invalidates this particular closure proof.
            if (observed - current) * header.step < 0 {
                return Err(not_proven());
            }
            offset = (observed - header.start) * header.step + 1;
            if header.assigns_outer {
                // Return/throw paths already captured the pre-update counter.
                locals.insert(header.counter.into(), counter_value(observed + header.step));
            }
            for (name, value) in env.iter_mut() {
                *value = locals[name].clone();
            }
            locals.retain(|name, _| env.contains_key(name));
        }
        Ok(())
    }

    pub(super) fn execute_source_try(
        &mut self,
        resource: Option<&S>,
        body: &[S],
        catches: &[CatchClause],
        finally: Option<&[S]>,
        id: &str,
        env: &mut Env,
        path: Expr,
    ) -> Result<()> {
        let parent = std::mem::replace(&mut self.realtime_completion, Completion::Continue);

        let prior_realtime_throws = self.realtime_throws.len();
        let mut locals = env.clone();
        let closing = if let Some(resource) = resource {
            let (binding, close) = crate::normalized::closed_resource_binding(self.program, resource)
                .ok_or_else(|| fail("try-with-resources needs a closed source allocation and close method"))?;
            self.source_steps(std::slice::from_ref(resource), id, &mut locals, path.clone())?;
            let close_path = self.normal_path(path.clone());
            if close_path.as_bool() == Some(false) { None } else {
                let value = locals.get(binding).ok_or_else(|| fail("resource binding not initialized"))?.clone();
                Some((close, value, close_path))
            }
        } else { None };
        self.source_steps(body, id, &mut locals, path.clone())?;
        if let Some((close, resource, close_path)) = closing {
            let protected_completion = std::mem::replace(&mut self.realtime_completion, Completion::Continue);
            // The protected completion already owns constructor/body
            // failures. They are pending during close, not close failures
            // that could erase this newly entered unwind path.
            self.realtime_throws.truncate(prior_realtime_throws);
            // An acquired resource closes on every body completion, even
            // when a return or throw is pending. The currently admitted
            // source slice must prove close itself cannot throw.
            self.java_call_on(&close, Vec::new(), close_path.clone(), Some(resource))?;
            if self.realtime_completion.clone().alternatives().iter().any(|alternative|
                matches!(alternative.outcome, CompletionOutcome::Throw { .. } | CompletionOutcome::ThrowValue(_))
                    && math::normalize(b::and(close_path.clone(), alternative.guard.clone())).as_bool() != Some(false))
            {
                return Err(fail("close with a possible failure requires suppressed-exception semantics"));
            }
            self.realtime_completion = protected_completion;
        }
        let mut pending = self.realtime_completion.clone().alternatives();

        self.realtime_throws.truncate(prior_realtime_throws);
        let mut handled = Vec::new();
        for catch in catches {
            // from_parsed_source has bound every handler, independent of its reachability.
            let caught = catch.exception_classes().iter()
                .map(|name| &self.project.source_exception_bindings[id][name])
                .collect::<Vec<_>>();
            let mut guard = b::lit_bool(false);
            pending.retain(|alternative| {
                if let CompletionOutcome::Throw { class, .. } = &alternative.outcome {
                    if caught.iter().any(|kind| crate::normalized::subtype(
                        class,
                        kind,
                        &self.project.source_exception_parents,
                    )) {
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
            pending.extend(handled);
            let saved = Completion::from_alternatives(pending);
            // A pending return or throw is not a closed execution path until
            // finally itself completes. Run the block once over the protected
            // frame's domain. Its abrupt completion overrides the saved one.
            self.realtime_throws.truncate(prior_realtime_throws);
            self.realtime_completion = Completion::Continue;
            let trailing_throw = finally.split_last().and_then(|(last, prefix)|
                crate::normalized::finally_throw(last)
                            .filter(|class| !matches!(*class, "RuntimeException" | crate::normalized::RUNTIME_EXCEPTION))
                            .map(|class| (class, prefix)));
            let body = trailing_throw.map_or(finally, |(_, prefix)| prefix);
            self.source_steps(body, id, &mut env.clone(), path.clone())?;
            if let Some((class, _)) = trailing_throw {
                let class = self.project.source_exception_bindings[id][class].clone();
                self.realtime_completion = self.realtime_completion.replace_continue(
                    Completion::Throw { class, payload: None },
                );
            }
            let finished = self.realtime_completion.replace_continue(saved);
            self.realtime_throws.truncate(prior_realtime_throws);
            self.realtime_throws.extend(finished.clone().alternatives().into_iter().filter_map(|alt| {
                matches!(alt.outcome, CompletionOutcome::Throw { .. } | CompletionOutcome::ThrowValue(_))
                    .then(|| math::normalize(b::and(path.clone(), alt.guard)))
            }));
            self.realtime_completion = parent.replace_continue(finished);
            return Ok(());
        }
        for CompletionAlternative { guard, outcome } in &pending {
            if matches!(outcome, CompletionOutcome::Throw { .. } | CompletionOutcome::ThrowValue(_)) {
                let escaped = math::normalize(b::and(path.clone(), guard.clone()));

                self.realtime_throws.push(escaped);
            }
        }
        pending.extend(handled);
        self.realtime_completion = parent.replace_continue(Completion::from_alternatives(pending));
        Ok(())
    }
}
