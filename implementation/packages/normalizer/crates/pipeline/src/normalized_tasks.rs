//! Pending task handles and guarded completions for settled business observations.
use super::*;
use actions::Action;
use realtime_state::Completion;

fn expression_is_closed(node: &Node) -> bool {
    match node {
        Node::Const { .. } | Node::Var { .. } => true,
        Node::Field { value, .. } | Node::Not { value } => expression_is_closed(value),
        Node::Add { left, right }
        | Node::Subtract { left, right }
        | Node::Gt { left, right }
        | Node::Eq { left, right } => expression_is_closed(left) && expression_is_closed(right),
        Node::If {
            cond,
            then,
            otherwise,
        } => [cond, then, otherwise]
            .into_iter()
            .all(|node| expression_is_closed(node)),
        _ => false,
    }
}

pub(super) fn validate_settled_scope(mut steps: &[Action]) -> Result<()> {
    while let Some((action, tail)) = steps.split_first() {
        steps = tail;
        match action {
            Action::ReadInput { .. } | Action::Rethrow | Action::LocalIr { .. } => {}
            Action::WriteState { value, .. }
            | Action::Let { value, .. }
            | Action::Return { value }
                if expression_is_closed(value) => {}
            Action::Catch { body, handler, .. } => {
                validate_settled_scope(body)?;
                validate_settled_scope(handler)?;
            }
            Action::TaskInvoke { name, args, .. } if args.iter().all(expression_is_closed) => {
                let Some((
                    Action::TaskWait {
                        task: Node::Var { name: awaited },
                        ..
                    },
                    tail,
                )) = steps.split_first()
                else {
                    return Err(fail("entry contains an outstanding task"));
                };
                need(
                    name == awaited,
                    "wait does not belong to the preceding invocation",
                )?;
                steps = tail;
            }
            _ => return Err(fail("unclosed operation in settled entry")),
        }
    }
    Ok(())
}

impl Engine<'_> {
    fn task_id(&self, task: &Value) -> Result<u64> {
        need(matches!(task.ty(), Ty::Task(_)), "expected a task handle")?;
        match task.scalar()?.as_sym_var() {
            Some(SymVarOrigin::Temporal { binding_id, .. }) => Ok(*binding_id),
            _ => Err(fail("task identity is not closed at this operation")),
        }
    }

    fn task_outcome(&self, task: &Value) -> Result<Completion> {
        let bind = self.task_id(task)?;
        let spec = self.pending_ports.get(&bind).ok_or_else(|| fail("unknown task handle"))?;
        let response = b::external_completion(bind);
        let mut outcome = Completion::Continue;
        for (class, guard, payload) in spec.throw_variants(&response) {
            outcome = outcome.replace_continue_with_throw(guard, class, payload);
        }
        Ok(outcome.replace_continue(Completion::Return(spec.awaited_value(&response)?)))
    }

    pub(super) fn task_action(
        &mut self,
        action: &Action,
        env: &mut Env,
    ) -> Result<()> {
        match action {
            Action::TaskInvoke {
                name,
                port,
                site,
                args,
            } => {
                need(
                    self.project
                        .ports
                        .get(port)
                        .is_some_and(|p| p.heap_effect == "none"),
                    "async port must have no hidden heap mutation",
                )?;
                let (args, ty) = self.invocation_arguments(port, site, args, env)?;
                let spec = self.project.ports[port].completion_spec(self.project)?;
                let bind = self.fresh_binding();
                self.pending_ports.insert(bind, spec);
                let guard = math::normalize(b::and(
                    self.action_path.clone(),
                    self.realtime_completion.continuing_guard(),
                ));
                self.realtime_effects.record(
                    guard,
                    port,
                    site,
                    args,
                    Some(b::external_completion(bind)),
                );
                Self::bind_local(
                    env,
                    name,
                    Value::Term(Ty::Task(Box::new(ty)), b::pending_task_handle(bind)),
                )?;
            }
            Action::TaskWait { name, task } => {
                need(
                    self.async_depth > 0,
                    "task_wait requires an async function body",
                )?;
                let task = self.action_value(task, env)?;
                let Ty::Task(ty) = task.ty() else {
                    return Err(fail("task_wait requires task<T>"));
                };
                let outcome = self.task_outcome(&task)?;
                self.suspension += 1;
                let value = outcome.normal_return_value().unwrap_or_else(b::lit_null);
                self.realtime_completion = self
                    .realtime_completion
                    .replace_continue(outcome.resume_after_normal_return());
                Self::bind_local(env, name, Value::Term(*ty, value))?;
            }
            _ => unreachable!("only task actions are dispatched here"),
        }
        Ok(())
    }
}
