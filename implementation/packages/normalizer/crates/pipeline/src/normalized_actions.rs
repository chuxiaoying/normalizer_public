//! Action lowering uses the SAME project loader, type environment and value
//! simulator as pure definitions. Only the effect carrier lives in the executor.
use super::*;

/// Adapters may declare only the runtime contract actually executed.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum PortHeapEffect {
    None,
}

#[derive(Debug)]
enum DeclaredPortType {
    Adapter(String),
    Source(Ty),
}

impl<'de> Deserialize<'de> for DeclaredPortType {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> std::result::Result<Self, D::Error> {
        String::deserialize(decoder).map(Self::Adapter)
    }
}

impl DeclaredPortType {
    fn resolve(&self) -> Result<Ty> {
        match self {
            Self::Adapter(typ) => Ty::parse(typ),
            Self::Source(typ) => {
                // Source records have one level of scalar fields. Retain the
                // field-name admission formerly enforced by the wire parser.
                if let Ty::Record(fields) = typ {
                    need(fields.iter().all(|(name, field)| values::valid_record_field(name, field)),
                        "invalid/duplicate record field")?;
                }
                Ok(typ.clone())
            }
        }
    }
}

#[derive(Debug)]
struct ImmutablePortObservation;

impl<'de> Deserialize<'de> for ImmutablePortObservation {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> std::result::Result<Self, D::Error> {
        match String::deserialize(decoder)?.as_str() {
            "immutable_value" => Ok(Self),
            _ => Err(serde::de::Error::custom("unsupported port observation contract")),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Port {
    pub(super) origin: Origin,
    pub(super) endpoint: String,
    params: Vec<Parameter<DeclaredPortType>>,
    returns: DeclaredPortType,
    throws: Vec<String>,
    #[serde(rename = "heap_effect")]
    _heap_effect: PortHeapEffect,
    #[serde(rename = "argument_observation")]
    _argument_observation: ImmutablePortObservation,
    effect: String,
}

impl Port {
    pub(super) fn from_source(
        origin: Origin,
        endpoint: String,
        params: Vec<Parameter<Ty>>,
        returns: Ty,
        throws: Vec<String>,
    ) -> Self {
        Self {
            origin,
            endpoint,
            params: params.into_iter().map(|param| Parameter {
                name: param.name, typ: DeclaredPortType::Source(param.typ),
            }).collect(),
            returns: DeclaredPortType::Source(returns),
            throws,
            _heap_effect: PortHeapEffect::None,
            _argument_observation: ImmutablePortObservation,
            effect: "sync".into(),
        }
    }

}
/// One admitted signature, used by both producers and the effect renderer.
pub(super) struct CheckedPort {
    pub(super) parameters: Vec<Ty>,
    pub(super) completion: port_completion::PortSpec,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Action {
    LocalIr {
        #[serde(rename = "source", deserialize_with = "read_relation_action")]
        query: Box<normalizer_ir::local::relation::Query>,
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
// The wire still carries the authored AIR string. Admit only relation
// actions at input; the executor receives an already parsed query.
fn read_relation_action<'de, D>(
    decoder: D,
) -> std::result::Result<Box<normalizer_ir::local::relation::Query>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use normalizer_ir::local::{parse, LocalIr};
    let source = String::deserialize(decoder)?;
    match parse(&source) {
        Ok(LocalIr::Relation(query)) => Ok(Box::new(query)),
        Ok(_) => Err(serde::de::Error::custom("expected a typed relation query")),
        Err(error) => Err(serde::de::Error::custom(format!(
            "expected a typed relation query: {error}"
        ))),
    }
}

impl Project {
    pub(super) fn validate_state_metadata(&self) -> Result<BTreeMap<String, CheckedPort>> {
        for (name, ty) in &self.heap_fields {
            need(identifier(name), "invalid heap field")?;
            let ty = Ty::parse(ty)?;
            need(ty.state_value(), "heap field needs a closed value type")?;
        }
        let mut exception_types = BTreeMap::new();
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
            let payload = if ty == "unit" {
                Ty::Unit
            } else {
                let ty = Ty::parse(ty)?;
                need(
                    ty.port_value(),
                    "exception payload needs a closed immutable value without references",
                )?;
                ty
            };
            exception_types.insert(name.as_str(), payload);
        }
        let mut ports = BTreeMap::new();
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
                need(ty.port_value(), "immutable port values cannot contain object references or source sequences")
            };
            let returns = p.returns.resolve()?;
            validate(&returns)?;
            let mut names = BTreeSet::new();
            let mut parameters = Vec::with_capacity(p.params.len());
            for a in &p.params {
                need(
                    identifier(&a.name) && names.insert(&a.name),
                    "invalid port parameter",
                )?;
                let ty = a.typ.resolve()?;
                validate(&ty)?;
                parameters.push(ty);
            }
            let mut exceptions = BTreeMap::new();
            for exception in &p.throws {
                need(
                    (builtin_exception(exception) || exception_types.contains_key(exception.as_str()))
                        && !exceptions.contains_key(exception),
                    "unknown/duplicate port exception",
                )?;
                let payload = exception_types.get(exception.as_str()).cloned().unwrap_or(Ty::Unit);
                exceptions.insert(exception.clone(), payload);
            }
            ports.insert(name.clone(), CheckedPort {
                parameters, completion: port_completion::PortSpec { returns, exceptions },
            });
        }
        Ok(ports)
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
fn adapter_scalar_domain(domain: &str) -> Option<Ty> {
    match domain {
        "bool" => Some(Ty::Bool),
        "number" => Some(Ty::Integer),
        _ => None,
    }
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
        returns: &Ty,
        steps: &[Action],
        env: &mut Env,
        path: Expr,
        settled: bool,
    ) -> Result<Value> {
        let parent_completion = std::mem::replace(
            &mut self.realtime_completion, realtime_state::Completion::Continue,
        );
        let caller_action_path = std::mem::replace(&mut self.action_path, path);
        let executed = self.action_steps(steps, &mut env.clone(), returns);
        self.action_path = caller_action_path;
        executed?;
        let local_completion = self.realtime_completion.clone();
        let terminal = local_completion.terminal_alternatives()
            .ok_or_else(|| fail("action has a normal path without method return"))?;
        let returned = terminal.normal_return_value();
        let completion = parent_completion.replace_continue(local_completion);
        // A normally completed helper is a value in the caller; a settled
        // entry, or a mixed/throwing helper, retains its completion.
        self.realtime_completion = if !settled && matches!(completion, realtime_state::Completion::Return(_)) {
            parent_completion
        } else {
            completion
        };
        match returned {
            Some(value) => Ok(Value::Term(returns.clone(), value)),
            // The caller has no live normal path on which to observe a value.
            None => Ok(Value::Term(returns.clone(), b::lit_null())),
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
    ) -> Result<Vec<Expr>> {
        let p = self
            .checked_ports
            .get(port)
            .ok_or_else(|| fail("undeclared external port"))?;
        need(
            identifier(site) && args.len() == p.parameters.len(),
            "invalid call site/arity",
        )?;
        // Evaluating an argument may update the engine's effects.
        let arg_types = p.parameters.clone();
        let mut values = Vec::new();
        for (arg, expected) in args.iter().zip(arg_types) {
            let v = self.action_value(arg, env)?;
            need(v.ty() == expected, "port argument type mismatch")?;
            values.push(v.scalar()?);
        }
        Ok(values)
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
        let checked = &self.checked_ports[id];
        let spec = &checked.completion;
        need(identifier(site) && contract.endpoint == target && contract.effect == "sync"
            && checked.parameters.is_empty()
            && spec.returns == Ty::Unit && !spec.throwing(),
            "listener call requires a declared normal zero-argument void sync port")?;
        let binding = self.fresh_binding();
        self.realtime_effects.record(guard, id, site, Vec::new(), realtime_state::EffectReceipt::tagged(binding));
        Ok(())
    }

    fn read_host_input(&mut self, env: &mut Env, name: &str, input: &str) -> Result<()> {
        need(identifier(input), "invalid host input name")?;
        let context = self.entry_context.ok_or_else(|| fail("host input requires the selected entry context"))?;
        let domain = context.parameter_type(input)
            .ok_or_else(|| fail(format!("host input {input} has no P declaration")))?;
        let ty = adapter_scalar_domain(domain)
            .ok_or_else(|| fail(format!("unsupported host input type: {domain}")))?;
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
    ) -> Result<()> {
        let entry_path = self.action_path.clone();
        for action in steps {
            self.action_path = math::normalize(b::and(
                entry_path.clone(),
                self.realtime_completion.continuing_guard(),
            ));
            self.budget()?;
            match action {
                Action::LocalIr { query } => {
                    self.execute_query(query, None, env, &self.action_path.clone())?;
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
                    let expected = adapter_scalar_domain(domain)
                        .ok_or_else(|| fail("unsupported state field type"))?;
                    let value = self.action_value(value, env)?;
                    need(value.ty() == expected, "incompatible state write value")?;
                    let value = value.scalar()?;
                    let guard = math::normalize(b::and(
                        self.action_path.clone(),
                        self.realtime_completion.continuing_guard(),
                    ));
                    // Adapter state writes have no readable heap identity; S owns them.
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
                }
                Action::Return { value } => {
                    let v = self.action_value(value, env)?;
                    need(v.ty() == *returns, "action return type mismatch")?;
                    let value = v.scalar()?;
                    self.realtime_completion = self.realtime_completion.replace_continue(
                        realtime_state::Completion::Return(value),
                    );
                    break;
                }
                Action::Rethrow => {
                    let caught = self
                        .caught_exceptions
                        .as_ref()
                        .ok_or_else(|| fail("rethrow requires an enclosing catch handler"))?;
                    self.realtime_completion = self.realtime_completion.replace_continue(
                        realtime_state::Completion::from_alternatives(caught.clone()),
                    );
                    break;
                }
            }
        }
        self.action_path = entry_path;
        Ok(())
    }
}

#[cfg(test)]
mod adapter_contract_tests {
    use super::*;
    use serde_json::{json, Value as JsonValue};

    fn project(steps: Vec<JsonValue>) -> JsonValue {
        json!({
            "schema": "normalizer.kernel-input/1",
            "source": "Carrier.java",
            "source_text": "class Entry { static native void run(); }",
            "entries": ["Entry.run"],
            "entry_contexts": {
                "Entry.run": {
                    "function": "Prepared.ts#run", "returns": "void", "parameters": {},
                    "external_results": { "notice": "void" }
                }
            },
            "ports": {
                "notice": {
                    "origin": { "path": "Prepared.ts", "excerpt": "notices.send()" },
                    "endpoint": "notices.send", "effect": "sync", "params": [],
                    "returns": "unit", "throws": [], "heap_effect": "none",
                    "argument_observation": "immutable_value"
                }
            },
            "functions": {
                "run": {
                    "java": "Entry.run()", "params": [], "returns": "unit",
                    "origin": { "path": "Prepared.ts", "excerpt": "notices.send()" },
                    "body": { "op": "settled", "returns": "unit", "steps": steps }
                }
            }
        })
    }

    fn unit_return() -> JsonValue {
        json!({ "op": "return", "value": { "op": "const", "type": "unit", "value": null } })
    }

    fn sync_call() -> JsonValue {
        json!({ "op": "call_sync", "port": "notice", "target": "notices.send", "site": "call1" })
    }

    fn at_project_boundary(doc: &JsonValue) -> Result<String> {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);

        struct Fixture(std::path::PathBuf);
        impl Drop for Fixture {
            fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); }
        }

        let path = std::env::temp_dir().join(format!(
            "normalizer-adapter-contract-{}-{}", std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        let fixture = Fixture(path);
        std::fs::write(fixture.0.join("Carrier.java"), doc["source_text"].as_str().unwrap()).unwrap();
        std::fs::write(fixture.0.join("Prepared.ts"), "notices.send()\n").unwrap();
        let input = fixture.0.join("Project.json");
        std::fs::write(&input, serde_json::to_vec(doc).unwrap()).unwrap();
        Loaded::project_pers(&input, "Entry.run")
    }

    #[test]
    fn listener_sync_admits_only_the_exact_declared_call() {
        let control = project(vec![sync_call(), unit_return()]);
        let pers = at_project_boundary(&control).expect("declared sync event has PERS");
        let pers: serde_yaml::Value = serde_yaml::from_str(&pers).unwrap();
        let events = pers["S"].as_sequence().expect("sync effect sequence");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["call"]["external"].as_str(), Some("notices.send"));

        for (field, replacement) in [
            ("endpoint", json!("other.send")),
            ("effect", json!("async-barrier")),
            ("params", json!([{ "name": "value", "type": "number" }])),
            ("returns", json!("number")),
        ] {
            let mut variant = control.clone();
            variant["ports"]["notice"][field] = replacement;
            let problem = at_project_boundary(&variant).unwrap_err();
            assert!(problem.contains("listener call requires a declared normal zero-argument void sync port"), "{field}: {problem}");
        }
    }

    #[test]
    fn builtin_throw_contract_does_not_authorize_an_invalid_invoke_site() {
        let mut doc = project(vec![
            json!({ "op": "task_invoke", "name": "task1", "port": "notice", "site": "bad-site", "args": [] }),
            json!({ "op": "task_wait", "name": "done", "task": { "op": "var", "name": "task1" } }),
            unit_return(),
        ]);
        doc["ports"]["notice"]["effect"] = json!("async-barrier");
        doc["ports"]["notice"]["throws"] = json!(["java.lang.IllegalArgumentException"]);

        let declared: Project = serde_json::from_value(doc.clone()).unwrap();
        let admitted = declared.validate_state_metadata().expect("built-in does not need a duplicate exception declaration");
        assert_eq!(
            admitted["notice"].completion.exceptions.get("java.lang.IllegalArgumentException"),
            Some(&Ty::Unit)
        );
        let problem = at_project_boundary(&doc).unwrap_err();
        assert!(problem.contains("invalid call site/arity"), "{problem}");
    }

    #[test]
    fn nonmatching_catch_neither_consumes_the_original_exception_nor_runs_handler() {
        let mut doc = project(vec![
            json!({
                "op": "catch", "classes": ["java.lang.NullPointerException"],
                "body": [
                    { "op": "task_invoke", "name": "task1", "port": "notice", "site": "task_site", "args": [] },
                    { "op": "task_wait", "name": "done", "task": { "op": "var", "name": "task1" } }
                ],
                "handler": [
                    { "op": "call_sync", "port": "unexpected", "target": "unexpected.send", "site": "handler_site" }
                ]
            }),
            unit_return(),
        ]);
        doc["ports"]["notice"]["effect"] = json!("async-barrier");
        doc["ports"]["notice"]["throws"] = json!(["java.lang.IllegalArgumentException"]);

        let pers = at_project_boundary(&doc).expect("nonmatching catch preserves original terminal alternatives");
        let pers: serde_yaml::Value = serde_yaml::from_str(&pers).unwrap();
        let events = pers["S"].as_sequence().expect("the one real task call");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["call"]["external"].as_str(), Some("notices.send"));
        let throws = pers["throws"].as_sequence().expect("the one uncaught class");
        assert_eq!(throws.len(), 1);
        assert_eq!(throws[0]["type"].as_str(), Some("java.lang.IllegalArgumentException"));
        assert_eq!(throws[0]["when"].as_str(), Some("completion.is_throw($e_0, java.lang.IllegalArgumentException)"));
    }

    #[test]
    fn invalid_lexical_local_fails_without_pers() {
        let doc = project(vec![
            json!({ "op": "let", "name": "bad-local", "value": { "op": "const", "type": "number", "value": 7 } }),
            unit_return(),
        ]);
        let problem = at_project_boundary(&doc).unwrap_err();
        assert!(problem.contains("action locals must be fresh lexical names"), "{problem}");
    }
}
