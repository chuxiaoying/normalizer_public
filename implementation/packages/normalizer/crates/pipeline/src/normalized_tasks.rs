//! Pending task handles and guarded completions for settled business observations.
use super::*;
use actions::Action;
use realtime_state::Completion;

pub(super) fn expression_is_closed(node: &Node) -> bool {
    match node {
        Node::Const(_) | Node::Var { .. } => true,
        Node::Field { value, .. } | Node::Not { value } => expression_is_closed(value),
        Node::Add { left, right }
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

#[derive(Clone, Debug, PartialEq)]
pub(super) struct SettledSteps(Vec<Action>);

impl SettledSteps {
    pub(super) fn as_slice(&self) -> &[Action] { &self.0 }
}

impl<'de> Deserialize<'de> for SettledSteps {
    fn deserialize<D: serde::Deserializer<'de>>(reader: D) -> std::result::Result<Self, D::Error> {
        let steps = Vec::<Action>::deserialize(reader)?;
        validate_settled_scope(&steps).map_err(serde::de::Error::custom)?;
        Ok(Self(steps))
    }
}

/// The synchronous adapter projection binds locals and returns a value.
pub(super) fn read_sync_steps<'de, D: serde::Deserializer<'de>>(
    reader: D,
) -> std::result::Result<Vec<Action>, D::Error> {
    let steps = Vec::<Action>::deserialize(reader)?;
    if steps.iter().any(|action| !matches!(action, Action::Let { .. } | Action::Return { .. })) {
        return Err(serde::de::Error::custom("synchronous projection accepts only local bindings and return"));
    }
    Ok(steps)
}

fn validate_settled_scope(mut steps: &[Action]) -> Result<()> {
    while let Some((action, tail)) = steps.split_first() {
        steps = tail;
        match action {
            Action::ReadInput { .. } | Action::Rethrow | Action::LocalIr { .. } => {}
            Action::CallSync { condition, .. }
                if condition.as_ref().is_none_or(expression_is_closed) => {}
            Action::WriteState { value, condition, .. }
                if expression_is_closed(value) && condition.as_ref().is_none_or(expression_is_closed) => {}
            Action::ThrowUninitializedCallback { condition: value }
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
                    return Err("entry contains an outstanding task".into());
                };
                if name != awaited {
                    return Err("wait does not belong to the preceding invocation".into());
                }
                steps = tail;
            }
            _ => return Err("unclosed operation in settled entry".into()),
        }
    }
    Ok(())
}

impl Engine<'_> {
    fn task_outcome(&self, bind: u64) -> Result<Completion> {
        let spec = self.pending_completions.get(&bind).ok_or_else(|| fail("unknown task handle"))?;
        let response = b::external_completion(bind);
        let mut outcome = Completion::Continue;
        for (class, guard, payload) in spec.throw_variants(&response) {
            outcome = outcome.replace_continue_with_throw(guard, class, payload);
        }
        Ok(outcome.replace_continue(Completion::Return(spec.tagged_value(&response)?)))
    }

    pub(super) fn task_invoke(
        &mut self,
        name: &str,
        port: &str,
        site: &str,
        args: &[Node],
        env: &mut Env,
    ) -> Result<()> {
        let args = self.invocation_arguments(port, site, args, env)?;
        let completion = self.checked_ports[port].completion.clone();
        let ty = completion.returns.clone();
        let bind = self.fresh_binding();
        self.pending_completions.insert(bind, completion.clone());
        let guard = math::normalize(b::and(
            self.action_path.clone(),
            self.realtime_completion.continuing_guard(),
        ));
        self.realtime_effects.record_port(
            guard,
            port,
            site,
            args,
            realtime_state::EffectReceipt::tagged(bind),
            completion,
        );
        Self::bind_local(
            env,
            name,
            Value::Term(Ty::Task(Box::new(ty)), b::pending_task_handle(bind)),
        )
    }

    pub(super) fn task_wait(&mut self, name: &str, task: &Node, env: &mut Env) -> Result<()> {
        let task = self.action_value(task, env)?;
        let Ty::Task(ty) = task.ty() else {
            return Err(fail("task_wait requires task<T>"));
        };
        let handle = task.scalar()?;
        let Some(SymVarOrigin::Temporal { binding_id, .. }) = handle.as_sym_var() else {
            return Err(fail("task identity is not closed at this operation"));
        };
        let outcome = self.task_outcome(*binding_id)?;
        let value = outcome.normal_return_value().unwrap_or_else(b::lit_null);
        self.realtime_completion = self
            .realtime_completion
            .replace_continue(outcome.resume_after_normal_return());
        Self::bind_local(env, name, Value::Term(*ty, value))
    }
}
