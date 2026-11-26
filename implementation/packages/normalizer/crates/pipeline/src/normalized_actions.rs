//! Action lowering uses the SAME project loader, type environment and value
//! simulator as pure definitions. Only the effect carrier lives in the executor.
use super::*;

/// Adapters may declare only the runtime contract actually executed.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum PortHeapEffect {
    None,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Port {
    pub(super) origin: Origin,
    pub(super) endpoint: String,
    params: Vec<Parameter>,
    returns: String,
    throws: Vec<String>,
    #[serde(rename = "heap_effect")]
    _heap_effect: PortHeapEffect,
    argument_observation: String,
    effect: String,
}

impl Port {
    pub(super) fn from_source(
        origin: Origin,
        endpoint: String,
        params: Vec<Parameter>,
        returns: String,
        throws: Vec<String>,
    ) -> Self {
        Self {
            origin,
            endpoint,
            params,
            returns,
            throws,
            _heap_effect: PortHeapEffect::None,
            argument_observation: "immutable_value".into(),
            effect: "sync".into(),
        }
    }

    pub(super) fn completion_spec(&self, project: &Project) -> Result<port_completion::PortSpec> {
        let exceptions = self
            .throws
            .iter()
            .map(|class| {
                let payload = if project.type_only_exception(class) {
                    Ty::Unit
                } else {
                    Ty::parse(
                        project
                            .exceptions
                            .get(class)
                            .ok_or_else(|| fail("unknown port exception"))?,
                    )?
                };
                Ok((class.clone(), payload))
            })
            .collect::<Result<_>>()?;
        Ok(port_completion::PortSpec {
            returns: Ty::parse(&self.returns)?,
            exceptions,
        })
    }


}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Action {
    LocalIr {
        source: String,
    },
    WriteState {
        target: Vec<String>,
        value: Node,
    },
    /// Access an immutable input bound for the selected invocation.
    ReadInput {
        name: String,
        input: String,
    },
    CallSync {
        port: Option<String>,
        target: String,
        site: String,
        condition: Option<Node>,
    },
    TaskInvoke {
        name: String,
        port: String,
        site: String,
        args: Vec<Node>,
    },
    TaskWait {
        name: String,
        task: Node,
    },
    Let {
        name: String,
        value: Node,
    },
    Catch {
        classes: Vec<String>,
        body: Vec<Action>,
        handler: Vec<Action>,
    },
    Return {
        value: Node,
    },
    Rethrow,
}
impl Project {
    pub(super) fn validate_state_metadata(&self) -> Result<()> {
        for (name, ty) in &self.heap_fields {
            need(identifier(name), "invalid heap field")?;
            let ty = Ty::parse(ty)?;
            need(ty.state_value(), "heap field needs a closed value type")?;
        }
        for (name, ty) in &self.exceptions {
            need(
                !name.is_empty()
                    && !matches!(
                        name.as_str(),
                        "java.lang.NullPointerException" | "java.lang.IndexOutOfBoundsException"
                    ),
                "invalid/reserved exception",
            )?;
            // unit observes only the exception class; no payload is invented.
            if ty != "unit" {
                let ty = Ty::parse(ty)?;
                need(
                        ty.port_value(),
                        "exception payload needs a closed immutable value without references",
                    )?;
            }
        }
        for (name, p) in &self.ports {
            need(
                (identifier(name) || self.source_ports.contains_key(name))
                    && !p.endpoint.trim().is_empty(),
                "invalid port/endpoint",
            )?;
            need(
                matches!(p.effect.as_str(), "sync" | "async-barrier"),
                "unsupported port effect; declare sync or async-barrier",
            )?;
            let validate = |ty: &Ty| -> Result<()> {
                match p.argument_observation.as_str() {
                    "immutable_value" =>
                        need(ty.port_value(), "immutable port values cannot contain object references or source sequences"),
                    "reference_value" =>
                        need(ty.state_value(), "reference port values require closed state types; source sequences must be explicit heap fields"),
                    _ => Err(fail("unsupported port observation contract")),
                }
            };
            validate(&Ty::parse(&p.returns)?)?;
            let mut names = BTreeSet::new();
            for a in &p.params {
                need(
                    identifier(&a.name) && names.insert(&a.name),
                    "invalid port parameter",
                )?;
                validate(&Ty::parse(&a.typ)?)?;
            }
            let mut exceptions = BTreeSet::new();
            for exception in &p.throws {
                need(
                    (builtin_exception(exception) || self.exceptions.contains_key(exception))
                        && exceptions.insert(exception),
                    "unknown/duplicate port exception",
                )?;
            }
        }
        Ok(())
    }
    pub(super) fn type_only_exception(&self, name: &str) -> bool {
        self.exceptions
            .get(name)
            .map_or_else(|| builtin_exception(name), |ty| ty == "unit")
    }
}
pub(super) fn builtin_exception(name: &str) -> bool {
    // The normalized action wire has one additional declared type with
    // no implicit source operation. Wire names remain fully qualified.
    name == "java.lang.IllegalArgumentException"
        || crate::normalized::builtin_exception_name(name) == Some(name)
}
impl Engine<'_> {
    pub(super) fn fresh_binding(&mut self) -> u64 {
        let id = self.next_binding;
        self.next_binding += 1;
        id
    }
    pub(super) fn action_value(&mut self, n: &Node, env: &mut Env) -> Result<Value> {
        self.node(n, env, self.action_path.clone())
    }
    pub(super) fn action_scope(
        &mut self,
        returns: &str,
        steps: &[Action],
        env: &mut Env,
        path: Expr,
        settled: bool,
    ) -> Result<Value> {
        let ty = Ty::parse(returns)?;
        let parent_completion = self.realtime_completion.clone();
        self.realtime_completion = realtime_state::Completion::Continue;
        // A normal helper scope does not become async merely because its
        // caller is async. A settled entry observes its own async completion.
        let caller_async_depth = self.async_depth;
        self.async_depth = usize::from(settled);
        let parent_settled = std::mem::replace(&mut self.settled_task_frame, settled);
        let caller_action_path = self.action_path.clone();
        self.action_path = path;
        let done = self.action_steps(steps, &mut env.clone(), &ty);
        self.async_depth = caller_async_depth;
        self.settled_task_frame = parent_settled;
        self.action_path = caller_action_path;
        done?;
        let local_completion = self.realtime_completion.clone();
        let terminal = local_completion.terminal_alternatives()
            .ok_or_else(|| fail("action has a normal path without method return"))?;
        let returned = terminal.normal_return_value();
        self.realtime_completion = parent_completion.replace_continue(local_completion);
        match returned {
            Some(value) => Ok(Value::Term(ty, value)),
            // The caller has no live normal path on which to observe a value.
            None => Ok(Value::Term(ty, b::lit_null())),
        }
    }

    pub(super) fn choose(&mut self, condition: Expr, yes: Value, no: Value) -> Result<Value> {
        if let Some(taken) = condition.as_bool() {
            return Ok(if taken { yes } else { no });
        }
        select(condition, yes, no)
    }

    /// Snapshot the observations that disqualify an expression from a
    /// side-effect-free and total operand position.
    pub(super) fn bind_local(env: &mut Env, name: &str, value: Value) -> Result<()> {
        need(
            identifier(name) && !env.contains_key(name),
            "action locals must be fresh lexical names",
        )?;
        env.insert(name.into(), value);
        Ok(())
    }
    pub(super) fn invocation_arguments(
        &mut self,
        port: &str,
        site: &str,
        args: &[Node],
        env: &mut Env,
    ) -> Result<(Vec<Expr>, Ty)> {
        let p = self
            .project
            .ports
            .get(port)
            .ok_or_else(|| fail("undeclared external port"))?;
        need(
            identifier(site) && args.len() == p.params.len(),
            "invalid call site/arity",
        )?;
        let arg_types = p
            .params
            .iter()
            .map(|p| Ty::parse(&p.typ))
            .collect::<Result<Vec<_>>>()?;
        let ty = Ty::parse(&p.returns)?;
        let mut values = Vec::new();
        for (index, (arg, expected)) in args.iter().zip(arg_types).enumerate() {
            let v = self.action_value(arg, env)?;
            need(
                v.ty() == expected,
                &format!(
                    "port argument type mismatch at {}[{}]: expected={} actual={}",
                    port,
                    index,
                    expected.describe(),
                    v.ty().describe()
                ),
            )?;
            values.push(v.scalar()?);
        }
        Ok((values, ty))
    }

    fn call_closed_sync(
        &mut self, port: Option<&str>, target: &str, site: &str,
        condition: Option<&Node>, env: &mut Env,
    ) -> Result<()> {
        let mut guard = self.action_path.clone();
        if let Some(condition) = condition {
            let condition = self.action_value(condition, env)?;
            need(condition.ty() == Ty::Bool, "sync call requires a Boolean guard")?;
            guard = math::normalize(b::and(guard, condition.scalar()?));
        }
        if guard.as_bool() == Some(false) { return Ok(()); }
        let entry = self.selected_entry.unwrap_or("standalone module");
        let id = port.unwrap_or_else(|| crate::preprocess::out_of_source(entry, target));
        let contract = self.project.ports.get(id).ok_or_else(|| fail("missing sync call contract"))?;
        let spec = contract.completion_spec(self.project)?;
        need(identifier(site) && contract.endpoint == target && contract.effect == "sync"
            && contract.params.is_empty()
            && spec.returns == Ty::Unit && !spec.throwing(),
            "listener call requires a declared normal zero-argument void sync port")?;
        let binding = self.fresh_binding();
        self.realtime_effects.record(guard, id, site, Vec::new(), Some(b::external_completion(binding)));
        Ok(())
    }

    fn read_host_input(&mut self, env: &mut Env, name: &str, input: &str) -> Result<()> {
        need(identifier(input), "invalid host input name")?;
        let context = self.entry_context.ok_or_else(|| fail("host input requires the selected entry context"))?;
        let domain = context.parameter_type(input)
            .ok_or_else(|| fail(format!("host input {input} has no P declaration")))?;
        let ty = match domain {
            "bool" => Ty::Bool,
            "number" => Ty::Integer,
            "text" => Ty::Text,
            _ => return Err(fail(format!("unsupported host input type: {domain}"))),
        };
        // P represents the entry snapshot, not a fresh host observation.
        // Reading it again aliases the same symbol even after an await.
        let value = symbolic(&host_input_symbol(input), &ty, &mut self.heap, None)?;
        Self::bind_local(env, name, value)
    }

    pub(super) fn action_steps(
        &mut self,
        steps: &[Action],
        env: &mut Env,
        returns: &Ty,
    ) -> Result<bool> {
        let entry_path = self.action_path.clone();
        let mut done = false;
        for action in steps {
            self.action_path = math::normalize(b::and(
                entry_path.clone(),
                self.realtime_completion.continuing_guard(),
            ));
            if self.action_path.as_bool() == Some(false) {
                break;
            }
            self.budget()?;
            need(!done, "unreachable action after completion")?;
            match action {
                Action::LocalIr { source } => {
                    let normalizer_ir::local::LocalIr::Relation(query) =
                        normalizer_ir::local::parse(source).map_err(|error| {
                            fail(format!("expected a typed relation query: {error}"))
                        })?
                    else {
                        return Err(fail("expected a typed relation query"));
                    };
                    self.execute_query(&query, None, env, &self.action_path.clone())?;
                }
                Action::WriteState { target, value } => {
                    let (root, fields) = target
                        .split_first()
                        .ok_or_else(|| fail("empty state write path"))?;
                    need(
                        target.iter().all(|part| identifier(part)),
                        "invalid state field path",
                    )?;
                    let context = self.entry_context.ok_or_else(|| fail("state write requires the selected entry context"))?;
                    let domain = context.state_field(root, fields).map_err(|detail| crate::preprocess::required(
                        "NORM-STATE-DECLARATION", &format!("{root}.{}", fields.join(".")), &detail,
                        "请在当前入口声明真实的状态字段和类型。",
                    ))?;
                    let expected = match domain {
                        "number" => Ty::Integer,
                        "bool" => Ty::Bool,
                        "text" => Ty::Text,
                        _ => return Err(fail("unsupported state field type")),
                    };
                    let value = self.action_value(value, env)?;
                    need(value.ty() == expected, "incompatible state write value")?;
                    let value = value.scalar()?;
                    let guard = math::normalize(b::and(
                        self.action_path.clone(),
                        self.realtime_completion.continuing_guard(),
                    ));
                    if guard.as_bool() == Some(false) {
                        continue;
                    }
                    let reference = self
                        .state_roots
                        .entry(root.clone())
                        .or_insert_with(|| {
                            b::heap_ref(
                                self.heap
                                    .new_path_object()
                                    .id(),
                            )
                        })
                        .clone();
                    self.realtime_store.store_guarded(
                        guard.clone(),
                        reference,
                        fields.join("."),
                        value.clone(),
                    );
                    self.realtime_effects.write_state(guard, target, value);
                }
                Action::ReadInput { name, input } => self.read_host_input(env, name, input)?,
                Action::CallSync { port, target, site, condition } => {
                    self.call_closed_sync(port.as_deref(), target, site, condition.as_ref(), env)?;
                }
                Action::TaskInvoke { name, port, site, args } => {
                    self.task_invoke(name, port, site, args, env)?;
                }
                Action::TaskWait { name, task } => self.task_wait(name, task, env)?,
                Action::Let { name, value } => {
                    let value = self.action_value(value, env)?;
                    Self::bind_local(env, name, value)?;
                }
                Action::Catch {
                    classes,
                    body,
                    handler,
                } => {
                    need(
                        !classes.is_empty(),
                        "catch requires explicit concrete exception classes",
                    )?;
                    let mut seen = std::collections::BTreeSet::new();
                    for class in classes {
                        need(seen.insert(class), "duplicate caught exception class")?;
                        need(self.project.type_only_exception(class),
                            "catch without payload binding accepts only declared type-only exceptions; preprocess observed exception payload explicitly")?;
                    }
                    let parent = std::mem::replace(
                        &mut self.realtime_completion,
                        realtime_state::Completion::Continue,
                    );
                    let path = self.action_path.clone();
                    let start_effects = self.realtime_effects.events().len();

                    let prior_realtime_throws = self.realtime_throws.len();
                    self.action_steps(body, &mut env.clone(), returns)?;
                    let (caught, mut exits): (Vec<_>,Vec<_>) = self.realtime_completion.clone().alternatives().into_iter().partition(|alt| {
                        matches!(&alt.outcome, realtime_state::CompletionOutcome::Throw {class,..} if classes.contains(class))
                    });

                    self.realtime_throws.truncate(prior_realtime_throws);
                    if !caught.is_empty() {
                        let guard = math::normalize(
                            caught
                                .iter()
                                .fold(b::lit_bool(false), |g, alt| b::or(g, alt.guard.clone())),
                        );
                        let mut local = env.clone();
                        let enclosing = self.caught_exceptions.replace(caught);
                        self.action_path = math::normalize(b::and(path.clone(), guard.clone()));
                        self.realtime_completion = realtime_state::Completion::Continue;
                        self.action_steps(handler, &mut local, returns)?;
                        self.caught_exceptions = enclosing;
                        for mut alt in self.realtime_completion.clone().alternatives() {
                            alt.guard = math::normalize(b::and(guard.clone(), alt.guard));
                            exits.push(alt);
                        }
                    }

                    self.realtime_throws.truncate(prior_realtime_throws);
                    for alt in &exits {
                        if matches!(alt.outcome, realtime_state::CompletionOutcome::Throw { .. }) {
                            let escaped = math::normalize(b::and(path.clone(), alt.guard.clone()));

                            self.realtime_throws.push(escaped);
                        }
                    }
                    self.action_path = path;
                    self.realtime_completion = parent
                        .replace_continue(realtime_state::Completion::from_alternatives(exits));
                    self.realtime_effects.merge_writes_since(start_effects);
                    done = self.realtime_completion.continuing_guard().as_bool() == Some(false);
                }
                Action::Return { value } => {
                    let v = self.action_value(value, env)?;
                    need(v.ty() == *returns, "action return type mismatch")?;
                    let value = v.scalar()?;
                    self.realtime_completion = self.realtime_completion.replace_continue(
                        realtime_state::Completion::Return(value.clone()),
                    );
                    done = true;
                }
                Action::Rethrow => {
                    let caught = self
                        .caught_exceptions
                        .as_ref()
                        .ok_or_else(|| fail("rethrow requires an enclosing catch handler"))?;
                    if !caught.is_empty() {
                        self.realtime_completion = self.realtime_completion.replace_continue(
                            realtime_state::Completion::from_alternatives(caught.clone()),
                        );
                    }
                    done = true;
                }
            }
        }
        self.action_path = entry_path;
        Ok(done)
    }
}
