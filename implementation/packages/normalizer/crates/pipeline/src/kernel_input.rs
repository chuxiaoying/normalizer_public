//! Typed, closed semantic definitions lowered into the existing math IR.
//! No library inference, executable plugins, opaque function summaries or samples
//! participate in normalization. Every supplied method/definition is checked.
use math::{
    builder as b,
    domains::relation::{self as r, RelationExpr},
    BinOpKind, Expr, SymVarOrigin,
};
use serde::Deserialize;
use crate::runtime_heap::Heap;
#[path = "normalized_actions.rs"]
mod actions;
#[path = "kernel_execution.rs"]
mod kernel_execution;
use kernel_execution::Engine;
#[path = "normalized_tasks.rs"]
mod tasks;
#[path = "normalized_values.rs"]
mod values;
use simulator_parser::{
    JavaBinOp as B, JavaExpr as J, JavaLit, JavaMethod, JavaProgram, JavaStatement as S, JavaType,
    JavaUnaryOp as U,
};
#[path = "realtime_state.rs"]
mod realtime_state;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    time::Instant,
};

type Result<T> = std::result::Result<T, String>;

/// Canonical wire schema for a closed input delivered to the normalizer core.
pub const KERNEL_INPUT_SCHEMA: &str = "normalizer.kernel-input/1";

fn is_kernel_input_schema(schema: &str) -> bool {
    schema == KERNEL_INPUT_SCHEMA
}

fn fail(message: impl AsRef<str>) -> String {
    crate::preprocess::required("NORM-PROJECT", "kernel-input/1", message.as_ref(),
        "按 implementation/skills/normalizer-preprocess/SKILL.md 交付闭合、类型正确的 Java 和语义定义；缺失能力须报告，不得增加黑盒、截断集合或删除异常。")
}
fn need(ok: bool, msg: &str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(fail(msg))
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Ty {
    Unit,
    Task(Box<Ty>),
    Bool,
    Integer,
    Decimal,
    I32,
    I64,
    Ref,
    Text,
    Seq(Box<Ty>),
    Record(BTreeMap<String, Ty>),
    Vector(Box<Ty>),
    OrderedTextMap,
    LocalTextMap,
    LocalRelation,
}
impl Ty {
    pub(crate) fn parse(s: &str) -> Result<Self> {
        need(
            s.chars().filter(|c| *c == '<').count() <= 4096,
            "structural type exceeds 4096 composite constructors",
        )?;
        if let Some(ty) = values::structural_type(s)? {
            return Ok(ty);
        }
        Ok(match s {
            "unit" => Self::Unit,
            "bool" => Self::Bool,
            "number" => Self::Integer,
            "i32" => Self::I32,
            "i64" => Self::I64,
            "ref?" => Self::Ref,
            "text?" => Self::Text,
            "map<text,text>" => Self::OrderedTextMap,
            _ => return Err(fail(format!("unsupported type {s}"))),
        })
    }
    fn java(t: &JavaType) -> Result<Self> {
        match t {
            JavaType::Void => Ok(Self::Unit),
            JavaType::Primitive(n) => Self::parse(match n.as_str() {
                "boolean" => "bool",
                "int" => "i32",
                "long" => "i64",
                _ => "unsupported",
            }),
            JavaType::Class(n) if n == "Object" || n == "java.lang.Object" => Ok(Self::Ref),
            JavaType::Class(n) if n == "String" => Ok(Self::Text),
            JavaType::Array(t) => Ok(Self::Seq(Box::new(match t.as_ref() {
                JavaType::Class(n) if n == "Object" => Self::Ref,
                _ => {
                    let t = Self::java(t)?;
                    need(
                        matches!(t, Self::I32 | Self::I64 | Self::Text),
                        "only integer, String or Object arrays",
                    )?;
                    t
                }
            }))),
            _ => Err(fail("unsupported Java type")),
        }
    }
}
#[derive(Clone, PartialEq)]
enum Value {
    Term(Ty, Expr),
    Rows(Box<business_relation::Rows>),
    LocalIntArray(source_arrays::LocalIntArray),
    LocalTextMap(source_map::LocalTextMap),
    EagerIntProjection { base: RelationExpr, selected: Expr },
    Sequence {
        item: Ty,
        java_component: Option<String>,
        relation: RelationExpr,
        value: Expr,
        null: Expr,
    },
}
fn primitive_array_component(ty: &JavaType) -> Option<&str> {
    let JavaType::Array(component) = ty else { return None; };
    let JavaType::Primitive(name) = component.as_ref() else { return None; };
    Some(name)
}

impl Value {
    fn java_array_component(&self) -> Option<&str> {
        match self {
            Self::LocalIntArray(_) => Some("int"),
            Self::Sequence { java_component, .. } => java_component.as_deref(),
            _ => None,
        }
    }

    fn check_java_array_component(&self, expected: Option<&str>) -> Result<()> {
        need(
            expected.zip(self.java_array_component()).is_none_or(|(expected, actual)| expected == actual),
            "Java array element type differs from the declared array",
        )
    }

    fn ty(&self) -> Ty {
        match self {
            Self::Term(t, _) => t.clone(),
            Self::Rows(_) => Ty::LocalRelation,
            Self::LocalTextMap(_) => Ty::LocalTextMap,
            Self::LocalIntArray(_) | Self::EagerIntProjection { .. } => Ty::Seq(Box::new(Ty::Integer)),
            Self::Sequence { item, .. } => Ty::Seq(Box::new(item.clone())),
        }
    }
    fn scalar(&self) -> Result<Expr> {
        match self {
            Self::Term(_, e) => Ok(e.clone()),
            _ => Err(fail("expected scalar")),
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    #[serde(default)]
    entry_contexts: BTreeMap<String, business_entry::EntryContext>,
    #[serde(skip)]
    direct_source: bool,
    #[serde(skip)]
    source_layouts: BTreeMap<String, BTreeMap<String, String>>,
    #[serde(skip)]
    source_ports: BTreeMap<String, crate::normalized::source_ports::SourcePort>,
    #[serde(skip)]
    source_exception_parents: BTreeMap<String, String>,
    schema: String,
    source: String,
    source_text: String,
    pub entries: Vec<String>,
    functions: BTreeMap<String, Function>,
    #[serde(default)]
    heap_fields: BTreeMap<String, String>,
    #[serde(default)]
    exceptions: BTreeMap<String, String>,
    #[serde(default)]
    ports: BTreeMap<String, actions::Port>,
    #[serde(default)]
    result_value_types: BTreeMap<String, String>,
    #[serde(default)]
    value_types: BTreeMap<String, String>,
    #[serde(default)]
    parameter_value_types: BTreeMap<String, BTreeMap<String, String>>,

}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Function {
    java: String,
    params: Vec<Parameter>,
    returns: String,
    origin: Origin,
    body: Node,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Parameter {
    name: String,
    #[serde(rename = "type")]
    typ: String,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Origin {
    path: String,
    excerpt: String,
}
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct OrderedMapEntry {
    key: String,
    input: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Node {
    #[serde(rename = "value.observe_tuple")]
    ObserveTuple {
        tuple: tuple_observation::Plan,
        observations: Vec<tuple_observation::Binding>,
        body: Box<Node>,
    },
    #[serde(rename = "value.number_vector")]
    NumberVector { items: Vec<Node> },
    #[serde(rename = "value.ordered_text_map")]
    OrderedTextMap { entries: Vec<OrderedMapEntry> },
    #[serde(rename = "value.field")]
    Field {
        value: Box<Node>,
        name: String,
    },
    /// Completes when the selected async entry settles. Nested regular
    /// functions and detached tasks retain their own completion boundaries.
    Settled {
        returns: String,
        steps: Vec<actions::Action>,
    },
    Action {
        returns: String,
        steps: Vec<actions::Action>,
    },
    Add {
        left: Box<Node>,
        right: Box<Node>,
    },
    Subtract {
        left: Box<Node>,
        right: Box<Node>,
    },
    Var {
        name: String,
    },
    Const {
        #[serde(rename = "type")]
        typ: String,
        value: serde_json::Value,
    },
    If {
        cond: Box<Node>,
        then: Box<Node>,
        #[serde(rename = "else")]
        otherwise: Box<Node>,
    },
    Not {
        value: Box<Node>,
    },
    Eq {
        left: Box<Node>,
        right: Box<Node>,
    },
    Gt {
        left: Box<Node>,
        right: Box<Node>,
    },
}
fn local_path(root: &Path, relative: &str) -> Result<PathBuf> {
    need(
        !Path::new(relative).is_absolute(),
        "source must be relative",
    )?;
    let p = root
        .join(relative)
        .canonicalize()
        .map_err(|e| fail(e.to_string()))?;
    need(p.starts_with(root), "source escapes project")?;
    Ok(p)
}
// A project carries complete source-defined semantics, including unused
// declarations. The entry closure remains lazy; input size is a resource guard.
const MAX_PROJECT_FILE_BYTES: u64 = 32 * 1024 * 1024;
fn read(path: &Path) -> Result<String> {
    need(
        std::fs::metadata(path)
            .map_err(|e| fail(e.to_string()))?
            .len()
            <= MAX_PROJECT_FILE_BYTES,
        "file exceeds 32 MiB",
    )?;
    std::fs::read_to_string(path).map_err(|e| fail(e.to_string()))
}
fn emit_stage_profile(stage: &str, started: Instant, detail: impl AsRef<str>) {
    if crate::profile::enabled() {
        crate::profile::event(
            "kernel_input.stage",
            &format!(
                "stage={stage} elapsed_us={} {}",
                started.elapsed().as_micros(),
                detail.as_ref()
            ),
        );
    }
}
pub struct Loaded {
    project: Project,
    program: JavaProgram,
}
impl Project {
    fn java_type(&self, t: &JavaType) -> Result<Ty> {
        if crate::normalized::integral(t) {
            return Ok(Ty::Integer);
        }
        if self.direct_source && crate::normalized::exact_decimal_type(t) {
            return Ok(Ty::Decimal);
        }
        if let JavaType::Array(item) = t {
            if self.direct_source && crate::normalized::integral(item) {
                return Ok(Ty::Seq(Box::new(Ty::Integer)));
            }
            if self.direct_source && matches!(item.as_ref(), JavaType::Array(element) if crate::normalized::integral(element)) {
                return Ok(Ty::Seq(Box::new(Ty::Seq(Box::new(Ty::Integer)))));
            }
            if self.direct_source && self.source_owner(item).is_some() {
                return Ok(Ty::Seq(Box::new(Ty::Ref)));
            }
        }
        Ty::java(t)
    }
    fn source_owner(&self, ty: &JavaType) -> Option<&str> {
        let JavaType::Class(name) = ty else {
            return None;
        };
        self.source_layouts
            .keys()
            .find(|owner| *owner == name || owner.rsplit('.').next() == Some(name.as_str()))
            .map(String::as_str)
    }
    fn parameter_type(&self, t: &JavaType) -> Result<Ty> {
        if matches!(t, JavaType::Class(n) if self.source_ports.values().any(|port| &port.interface == n)) {
            return Ok(Ty::Ref);
        }
        if self.source_owner(t).is_some() {
            return Ok(Ty::Ref);
        }
        if let JavaType::Class(name) = t {
            if let Some(value) = self.value_types.get(name) {
                return Ty::parse(value);
            }
        }
        self.java_type(t)
    }
    fn semantic_parameter_type(&self, method: &str, name: &str, t: &JavaType) -> Result<Ty> {
        if let Some(ty) = self
            .parameter_value_types
            .get(method)
            .and_then(|params| params.get(name))
        {
            return Ty::parse(ty);
        }
        self.parameter_type(t)
    }
    fn result_type(&self, t: &JavaType) -> Result<Ty> {
        if self.source_owner(t).is_some() {
            return Ok(Ty::Ref);
        }
        if let JavaType::Class(name) = t {
            if let Some(value) = self
                .result_value_types
                .get(name)
                .or_else(|| self.value_types.get(name))
            {
                return Ty::parse(value);
            }
        }
        self.java_type(t)
    }
}
impl Loaded {
    /// Closed source entrance: consume the exact AST validated by the source
    /// capability gate. No YAML roundtrip, native stand-in or general executor.
    pub fn from_parsed_source(program: JavaProgram, entry: &str) -> Result<Self> {
        crate::normalized::validate(&program, entry)?;
        let source_layouts = crate::normalized::source_layouts(&program)?;
        let source_exception_parents = crate::normalized::source_exceptions(&program)?;
        let source_ports = crate::normalized::source_ports::declarations(&program, &source_exception_parents)?;
        let ports = source_ports::contracts(&program, &source_ports)?;
        let exceptions = source_exception_parents
            .keys()
            .cloned()
            .map(|name| {
                let domain = program.find_class(&name).and_then(crate::normalized::source_payload_domain).unwrap_or("unit");
                (name, domain.to_owned())
            })
            .collect();
        let heap_fields = source_layouts
            .values()
            .flat_map(|fields| fields.iter())
            // A source array is a bound identity, not a scalar heap slot.
            .filter(|(_, ty)| ty.as_str() != "seq<number>")
            .map(|(field, ty)| (field.clone(), ty.clone()))
            .collect();
        let project = Project {
            direct_source: true,
            entry_contexts: BTreeMap::new(),
            source_layouts,
            source_ports,
            source_exception_parents,
            schema: KERNEL_INPUT_SCHEMA.into(),
            source: "<parsed-java-source>".into(),
            source_text: String::new(),
            entries: vec![entry.into()],
            functions: BTreeMap::new(),
            heap_fields,
            exceptions,
            ports,
            result_value_types: BTreeMap::new(),
            value_types: BTreeMap::new(),
            parameter_value_types: BTreeMap::new(),
        };
        // The source gate already checked syntax, calls and cycles. Only
        // normalize(entry) executes code, with the actual entry arguments and
        // receiver; auditing a constructor as a stand-alone call has no meaning.
        Ok(Self { project, program })
    }
    /// Load the closed adapter project without executing unrelated methods.
    pub fn load_for_entry(path: &Path) -> Result<Self> {
        let load_started = Instant::now();
        let path = path.canonicalize().map_err(|e| fail(e.to_string()))?;
        let root = path.parent().ok_or_else(|| fail("missing project root"))?;
        let project_text = read(&path)?;
        emit_stage_profile(
            "project_read",
            load_started,
            format!("input_bytes={}", project_text.len()),
        );
        let project: Project = serde_json::from_str(&project_text).map_err(|error| fail(error.to_string()))?;
        need(project.entries.len() == project.entry_contexts.len()
                && project.entries.iter().all(|name| project.entry_contexts.contains_key(name)),
                "business source contexts must cover each selected entry")?;
        emit_stage_profile(
            "project_deserialize",
            load_started,
            format!("functions={}", project.functions.len()),
        );
        if !is_kernel_input_schema(&project.schema) {
            return Err(fail("unsupported schema"));
        }
        need(!project.entries.is_empty(), "no entries")?;
        need(
            project
                .value_types
                .keys()
                .all(|name| !project.result_value_types.contains_key(name)),
            "duplicate composite value alias",
        )?;
        // A raw DAO/framework SQL port is still an opaque execution request.
        // The normalized kernel only accepts the already-preprocessed relation
        // root and its relational operators.  Keeping this guard at admission
        // prevents SQL text, mapper names, and driver behavior from leaking
        // into the boundary ledger as if they were ordinary business calls.
        if let Some((port, endpoint)) = project.ports.iter().find_map(|(name, port)| {
            port.endpoint
                .strip_prefix("sql.")
                .map(|_| (name, &port.endpoint))
        }) {
            return Err(fail(format!(
                "SQL port `{port}` ({endpoint}) requires AI preprocessing into relation semantics; raw SQL execution is not a normalized-kernel input"
            )));
        }
        let source = read(&local_path(root, &project.source)?)?;
        emit_stage_profile(
            "source_read",
            load_started,
            format!("source_bytes={}", source.len()),
        );
        need(
            source == project.source_text,
            "normalized source changed; review source_text",
        )?;
        for origin in project
            .functions
            .values()
            .map(|f| &f.origin)
            .chain(project.ports.values().map(|p| &p.origin))
        {
            let original = read(&local_path(root, &origin.path)?)?;
            need(
                !origin.excerpt.trim().is_empty() && original.contains(&origin.excerpt),
                "origin excerpt missing or changed",
            )?;
        }
        let program = simulator_parser::parse(&source, &project.source, &Default::default())
            .map_err(|e| fail(e.to_string()))?;
        emit_stage_profile(
            "source_parse",
            load_started,
            format!("classes={}", program.classes().len()),
        );
        let loaded = Self { project, program };
        let prepare_started = Instant::now();
        let e = loaded.prepare()?;
        for id in &loaded.project.entries {
            need(e.methods.contains_key(id), "entry missing")?;
        }
        emit_stage_profile(
            "prepare",
            prepare_started,
            format!("methods={}", e.methods.len()),
        );
        Ok(loaded)
    }
    fn engine(&self) -> Engine<'_> {
        Engine {
            node_depth: 0,
            max_node_depth: 0,
            program: &self.program,
            source_objects: source_objects::SourceObjects::default(),
            source_relations: Default::default(),
            temporal_definitions: Vec::new(),
            source_array_writes: Default::default(),
            project: &self.project,
            methods: BTreeMap::new(),
            bindings: BTreeMap::new(),
            active: BTreeSet::new(),
            steps: 0,
            async_depth: 0,
            settled_task_frame: false,
            selected_entry: None,
            entry_context: None,
            state_roots: BTreeMap::new(),
            pending_ports: BTreeMap::new(),
            caught_exceptions: None,
            next_binding: 0,
            realtime_local_allocations: BTreeSet::new(),
            heap: Heap::new(),
            action_path: b::lit_bool(true),
            semantic_calls: 0,
            java_calls: 0,
            node_visits: 0,
            branch_splits: 0,
            branch_collapses: 0,
            max_active_calls: 0,
            budget_reported: false,
            realtime_store: realtime_state::SymbolicStore::default(),
            realtime_effects: realtime_state::EffectSequence::default(),
            realtime_completion: realtime_state::Completion::Continue,
            realtime_throws: Vec::new(),
        }
    }
    fn prepare(&self) -> Result<Engine<'_>> {
        let mut e = self.engine();
        self.project.validate_state_metadata()?;
        let mut classes = BTreeSet::new();
        for c in self.program.classes() {
            need(
                !simulator_parser::is_reserved_identifier(c.simple_name()),
                "reserved Java class name; rename during preprocessing",
            )?;
            need(classes.insert(c.full_name()), "duplicate class")?;
            need(
                (c.supertype().is_none()
                        || self
                            .project
                            .source_exception_parents
                            .contains_key(&c.full_name()))
                    && (c.interfaces().is_empty() || self.project.direct_source)
                    && (!c.is_interface() || self.project.source_ports.values().any(|p| p.interface==c.full_name())
                        || (self.project.direct_source && crate::normalized::plain_source_interface(c)))
                    && (c.fields().is_empty()
                        || self.project.source_exception_parents.contains_key(&c.full_name())
                        || self.project.source_layouts.contains_key(&c.full_name()))
                    && c.annotations().is_empty()

                    && (c.imports().is_empty() || crate::normalized::supported_source_imports(c))
                    && !c.has_unbound_imports(),
                "unsupported class shape",
            )?;
            if let Some(value) = self
                .project
                .result_value_types
                .get(&c.full_name())
                .or_else(|| self.project.value_types.get(&c.full_name()))
            {
                let ty = Ty::parse(value)?;
                need(!matches!(c.full_name().as_str(), "Object" | "String")
                    && identifier(&c.full_name()) && c.methods().is_empty()
                    && matches!(ty, Ty::Task(_) | Ty::Vector(_) | Ty::Record(_) | Ty::OrderedTextMap) && ty.state_value(),
                    "result value alias requires a declared empty carrier class and composite state type")?;
            }
            if c.is_interface() {
                continue;
            }
            for m in c.methods() {
                need(
                    !simulator_parser::is_reserved_identifier(m.name()),
                    "reserved Java method name; rename during preprocessing",
                )?;
                need(
                    m.annotations().is_empty() || self.project.direct_source && crate::normalized::nullable_text_return(c, m),
                    "unsupported method shape",
                )?;
                let id = format!("{}.{}", c.full_name(), m.name());
                need(!e.methods.contains_key(&id), "duplicate/overloaded method")?;
                self.project.result_type(m.return_type())?;
                for (t, name) in m.params() {
                    need(
                        !simulator_parser::is_reserved_identifier(name),
                        "reserved Java parameter name; rename during preprocessing",
                    )?;
                    self.project.parameter_type(t)?;
                }
                e.methods.insert(id, m);
            }
        }
        for name in self
            .project
            .result_value_types
            .keys()
            .chain(self.project.value_types.keys())
        {
            need(
                classes.contains(name.as_str()),
                "result value carrier class is missing",
            )?;
        }
        for (name, f) in &self.project.functions {
            let (id, signature) = f
                .java
                .split_once('(')
                .ok_or_else(|| fail("binding requires full signature"))?;
            let m = *e
                .methods
                .get(id)
                .ok_or_else(|| fail(format!("binding has no Java declaration: {id}")))?;
            need(signature.ends_with(')'), "invalid signature")?;
            let java_types: Vec<_> = m
                .params()
                .iter()
                .map(|(t, _)| java_type_text(t))
                .collect::<Result<_>>()?;
            need(
                signature == format!("{})", java_types.join(",")),
                "Java parameter signature mismatch",
            )?;
            need(
                m.body().is_none(),
                "semantic binding cannot replace a Java body",
            )?;
            need(
                f.params.len() == m.params().len()
                    && Ty::parse(&f.returns)? == self.project.result_type(m.return_type())?,
                "semantic return/arity mismatch",
            )?;
            let mut names = BTreeSet::new();
            for (p, (t, _)) in f.params.iter().zip(m.params()) {
                need(
                    names.insert(&p.name) && identifier(&p.name),
                    "duplicate/invalid parameter",
                )?;
                need(
                    Ty::parse(&p.typ)?
                        == self.project.semantic_parameter_type(
                            id.rsplit('.').next().unwrap_or(id),
                            &p.name,
                            t,
                        )?,
                    "semantic parameter type mismatch",
                )?;
            }
            need(
                e.bindings.insert(id.to_string(), name.clone()).is_none(),
                "duplicate binding",
            )?;
        }
        for (id, m) in &e.methods {
            need(
                m.body().is_some() || e.bindings.contains_key(id)
                    || self.project.direct_source && m.is_native(),
                "unbound bodyless method",
            )?;
        }
        Ok(e)
    }
    fn normalize(&self, entry: &str) -> Result<ExecutionResult> {
        let selected = self.project.entries.iter().find(|s| *s == entry)
            .ok_or_else(|| fail("entry not declared"))?;
        let mut e = self.prepare()?;
        e.selected_entry = Some(selected.as_str());
        e.entry_context = self.project.entry_contexts.get(entry);
        let m = *e.methods.get(entry).ok_or_else(|| fail("entry method missing"))?;
        let args = m
            .params()
            .iter()
            .map(|(t, n)| {
                e.entry_argument(
                    n, t,
                    &self.project.semantic_parameter_type(
                        entry.rsplit('.').next().unwrap_or(entry),
                        n,
                        t,
                    )?,
                    crate::normalized::source_nullable_parameter(m, n, t)?,
                )
            })
            .collect::<Result<_>>()?;
        let entry_started = Instant::now();
        let value = e.java_call(entry, args, b::lit_bool(true))?;
        e.emit_stage_profile(
            "entry_execute",
            entry_started,
            "completed_entry_execute",
            "",
        );
        let source_record_return = if e.realtime_completion.is_always_throwing() {
            None
        } else {
            e.source_record(m.return_type(), &value)?
        };
        let realtime_completion = e.realtime_completion.replace_continue(
            realtime_state::Completion::Return(value.scalar()?),
        );
        need(realtime_completion.terminal_alternatives().is_some(), "entry has incomplete control flow")?;
        e.emit_profile_metrics("completed");
        Ok(ExecutionResult {
            temporal_definitions: e.temporal_definitions,
            source_state_bindings: e.source_objects.state_bindings,
            source_identity_parameters: e.source_objects.compared_parameters,
            source_identity_states: e.source_objects.compared_states,
            source_record_return,
            host_state_roots: e.state_roots.values().filter_map(Expr::as_heap_ref).collect(),
            source_tables: e.source_relations.tables,
            realtime_store: e.realtime_store,
            realtime_local_allocations: e.realtime_local_allocations,
            realtime_effects: e.realtime_effects,
            realtime_completion,
        })
    }
}
mod pure_fold;
mod member_choice;
mod temporal_graph;
mod business_entry;
mod business_event;
pub(crate) mod business_relation;
mod source_control;
pub(crate) mod port_completion;
mod business_sql;
mod source_objects;
mod source_arrays;
mod source_map;
mod source_stack;
mod tuple_observation;
mod source_decimal;
pub mod standalone_closure;
mod source_ports;
#[path = "kernel_result.rs"]
mod kernel_result;
use kernel_result::ExecutionResult;

fn identifier(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .enumerate()
            .all(|(i, c)| c == b'_' || c.is_ascii_alphabetic() || (i > 0 && c.is_ascii_digit()))
}
fn java_type_text(t: &JavaType) -> Result<String> {
    match t {
        JavaType::Void => Ok("void".to_string()),
        JavaType::Primitive(n) => Ok(n.clone()),
        JavaType::Class(n) if identifier(n) || n == "java.lang.Object" => Ok(n.clone()),
        JavaType::Array(t) => Ok(format!("{}[]", java_type_text(t)?)),
        _ => Err(fail("unsupported signature type")),
    }
}
// Unlike an external receipt, the declared initial value of an entry-state
// scalar is available on every path. Keep it a typed math input in a separate
// namespace and project it to E, never to a public entry parameter.
fn source_state_symbol(name: &str) -> String {
    format!("$initial_state.{name}")
}
fn host_input_symbol(name: &str) -> String {
    format!("$host.{name}")
}

fn temporal_definition_id(expr: &Expr) -> Option<u64> {
    match expr.as_sym_var() {
        Some(SymVarOrigin::Temporal { binding_id, primitive_type })
            if matches!(primitive_type.as_str(), "integer" | b::RECORD_TEMPORAL_TAG) => Some(*binding_id),
        _ => None,
    }
}

fn is_record_choice(expr: &Expr) -> bool {
    matches!(expr.as_sym_var(), Some(SymVarOrigin::Temporal { primitive_type, .. })
        if primitive_type == b::RECORD_TEMPORAL_TAG)
}

fn record_choice_projection(expr: &Expr) -> Option<&Expr> {
    let (key, args) = expr.as_call()?;
    let [record, field] = args else { return None; };
    math::domains::composite::field_intrinsic_result(key)?;
    matches!(field.as_lit(), Some(math::Lit::Str(_))).then_some(())?;
    is_record_choice(record).then_some(record)
}

fn integer_definition_id(expr: &Expr) -> Option<u64> {
    match expr.as_sym_var() {
        Some(SymVarOrigin::Temporal { binding_id, primitive_type }) if primitive_type == "integer" => Some(*binding_id),
        _ => None,
    }
}

fn symbolic(name: &str, t: &Ty, heap: &mut Heap, declared: Option<&JavaType>) -> Result<Value> {
    Ok(match t {
        Ty::Unit => Value::Term(Ty::Unit, b::lit_null()),
        Ty::Task(_) | Ty::Vector(_) | Ty::Record(_) => Value::Term(
            t.clone(),
            math::domains::composite::apply(
                "value.input",
                vec![b::lit_str(name)],
                vec![math::Primitive::String],
                math::Primitive::Composite,
            ),
        ),
        Ty::Seq(item) => Value::Sequence {
            item: *item.clone(),
            java_component: declared.and_then(primitive_array_component).map(str::to_owned),
            relation: RelationExpr::base(name, format!("{name}.item")),
            value: b::row_value(format!("{name}.item")),
            null: b::eq(
                b::heap_ref(heap.new_path_object().id()),
                b::lit_null(),
            ),
        },
        Ty::Ref => Value::Term(
            Ty::Ref,
            b::heap_ref(heap.new_path_object().id()),
        ),
        Ty::Bool => Value::Term(t.clone(), b::param(name, "boolean")),
        Ty::Integer => Value::Term(t.clone(), b::param(name, "integer")),
        Ty::Decimal => Value::Term(t.clone(), b::param(name, math::domains::decimal::VALUE_TAG)),
        Ty::I32 => Value::Term(t.clone(), b::param(name, "i32")),
        Ty::I64 => Value::Term(t.clone(), b::param(name, "i64")),
        Ty::Text => Value::Term(t.clone(), b::param(name, "text")),
        Ty::OrderedTextMap | Ty::LocalTextMap | Ty::LocalRelation =>
            return Err(fail("this value type has no input symbol contract")),
    })
}
type Env = BTreeMap<String, Value>;

fn select(c: Expr, yes: Value, no: Value) -> Result<Value> {
    need(
        yes.ty() == no.ty(),
        &format!(
            "branch result type mismatch: then={} else={}",
            yes.ty().describe(),
            no.ty().describe()
        ),
    )?;
    let (yes, no) = match (yes, no) {
        (Value::LocalTextMap(yes), Value::LocalTextMap(no)) =>
            return Ok(Value::LocalTextMap(yes.join(c, no))),
        other => other,
    };
    let ty = yes.ty();
    if ty == Ty::Integer {
        if let Some(extremum) = math::case_orientation::ordered_number_selection(
            &c, &yes.scalar()?, &no.scalar()?, &math::DefaultSortContext,
        ) {
            return Ok(Value::Term(ty, math::normalize(extremum)));
        }
    }
    let completion = realtime_state::join_completion(
        c,
        realtime_state::Completion::Return(yes.scalar()?),
        realtime_state::Completion::Return(no.scalar()?),
    )
    .ok_or_else(|| fail("branch completion cannot be joined at this continuation"))?;
    let realtime_state::Completion::Return(value) = completion else {
        return Err(fail("pure value branch produced a non-return completion"));
    };
    Ok(Value::Term(ty, value))
}
