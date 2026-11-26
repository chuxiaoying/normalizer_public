//! Typed, closed semantic definitions lowered into the existing math IR.
//! No library inference, executable plugins, opaque function summaries or samples
//! participate in normalization. Every supplied method/definition is checked.
use math::{
    builder as b,
    domains::relation::{self as r, RelationExpr},
    BinOpKind, Expr, ScalarSymbolType, SymVarOrigin,
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
    rc::Rc,
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
            "ref?" => Self::Ref,
            "text?" => Self::Text,
            "map<text,text>" => Self::OrderedTextMap,
            _ => return Err(fail(format!("unsupported type {s}"))),
        })
    }

}
#[derive(Clone, PartialEq)]
enum Value {
    Term(Ty, Expr),
    /// A host-proven lower bound, never inferred from the displayed Expr.
    IntegerFloor { value: Expr, lower: math::Integer, domain: Expr },
    Rows(Box<business_relation::Rows>),
    LocalIntArray(source_arrays::LocalIntArray),
    LocalTextMap(source_map::LocalTextMap),
    EagerIntProjection { base: RelationExpr, selected: Expr },
    Sequence {
        item: Ty,
        java_component: Option<String>,
        /// Identity of the acquired array, unchanged by lexical aliasing.
        name: String,
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
            Self::IntegerFloor { .. } => Ty::Integer,
            Self::Rows(_) => Ty::LocalRelation,
            Self::LocalTextMap(_) => Ty::LocalTextMap,
            Self::LocalIntArray(_) | Self::EagerIntProjection { .. } => Ty::Seq(Box::new(Ty::Integer)),
            Self::Sequence { item, .. } => Ty::Seq(Box::new(item.clone())),
        }
    }
    fn certified_integer_floor(&self, path: &Expr) -> Option<math::Integer> {
        match self {
            Self::IntegerFloor { lower, domain, .. }
                if path.as_bool() != Some(false) && b::path_implies(path, domain) => Some(lower.clone()),
            Self::Term(Ty::Integer, value) => value.as_lit()?.as_integer(),
            _ => None,
        }
    }

    fn scalar(&self) -> Result<Expr> {
        match self {
            Self::Term(_, e) | Self::IntegerFloor { value: e, .. } => Ok(e.clone()),
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
    #[serde(skip)]
    source_exception_bindings: crate::normalized::ExceptionBindings,
    #[serde(skip)]
    source_nullable_entry_parameters: BTreeSet<String>,
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
    parameter_value_types: BTreeMap<String, BTreeMap<String, String>>,

}
struct AdmittedMethod<'a> {
    syntax: &'a JavaMethod,
    defaults: Vec<Ty>,
    returns: Ty,
    binding: Option<Rc<CheckedBinding<'a>>>,
}

impl AdmittedMethod<'_> {
    fn parameter_types(&self) -> &[Ty] {
        self.binding.as_ref().map_or(self.defaults.as_slice(), |binding| &binding.parameters)
    }
}

struct CheckedBinding<'a> {
    name: &'a str,
    function: &'a Function,
    parameters: Vec<Ty>,
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
struct Parameter<T = String> {
    name: String,
    #[serde(rename = "type")]
    typ: T,
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireConstant {
    #[serde(rename = "type")]
    typ: String,
    value: serde_json::Value,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(try_from = "WireConstant")]
struct AdmittedConstant {
    ty: Ty,
    value: Expr,
}

impl TryFrom<WireConstant> for AdmittedConstant {
    type Error = String;

    fn try_from(raw: WireConstant) -> Result<Self> {
        let ty = Ty::parse(&raw.typ)?;
        let value = match (&ty, raw.value) {
            (Ty::Unit, serde_json::Value::Null) => b::lit_null(),
            (Ty::Bool, serde_json::Value::Bool(value)) => b::lit_bool(value),
            (Ty::Integer, serde_json::Value::Number(value)) =>
                b::lit_int(value.as_i64().ok_or("integer range")?),
            (Ty::Text, serde_json::Value::String(value)) => b::lit_str(value),
            _ => return Err("invalid constant".into()),
        };
        Ok(Self { ty, value })
    }
}

fn read_action_return<'de, D: serde::Deserializer<'de>>(reader: D) -> std::result::Result<Ty, D::Error> {
    Ty::parse(&String::deserialize(reader)?).map_err(serde::de::Error::custom)
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Node {
    #[serde(rename = "value.observe_tuple")]
    ObserveTuple(tuple_observation::Observation),
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
        #[serde(deserialize_with = "read_action_return")]
        returns: Ty,
        steps: tasks::SettledSteps,
    },
    Action {
        #[serde(deserialize_with = "read_action_return")]
        returns: Ty,
        #[serde(deserialize_with = "tasks::read_sync_steps")]
        steps: Vec<actions::Action>,
    },
    Add {
        left: Box<Node>,
        right: Box<Node>,
    },
    Var {
        name: String,
    },
    Const(AdmittedConstant),
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
    source_entry: Option<source_entry::SourceEntry>,
}
impl Project {
    fn java_type(&self, t: &JavaType) -> Result<Ty> {
        if let Some((ty, _)) = java_carrier_atom(t) {
            return Ok(ty);
        }
        if self.direct_source {
            // Java source admission recognizes the qualified builtin. The
            // generated adapter carrier ABI still requires its ordinary spelling.
            if matches!(t, JavaType::Class(name) if name == "java.lang.String") {
                return Ok(Ty::Text);
            }
            if crate::normalized::exact_decimal_type(t) || crate::normalized::native_decimal_type(t) {
                return Ok(Ty::Decimal);
            }
        }
        match t {
            JavaType::Void => Ok(Ty::Unit),
            JavaType::Array(item) => {
                let ty = match item.as_ref() {
                    _ if self.direct_source && crate::normalized::integral(item) => Ty::Integer,
                    JavaType::Array(element) if self.direct_source && crate::normalized::integral(element) =>
                        Ty::Seq(Box::new(Ty::Integer)),
                    _ if self.direct_source && self.source_owner(item).is_some() => Ty::Ref,
                    _ => return Err(fail("unsupported Java array type")),
                };
                Ok(Ty::Seq(Box::new(ty)))
            }
            _ => Err(fail("unsupported Java type")),
        }
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
        self.java_type(t)
    }
    fn parameter_override(&self, method: &str, name: &str) -> Result<Option<Ty>> {
        self.parameter_value_types.get(method).and_then(|params| params.get(name))
            .map(|ty| Ty::parse(ty)).transpose()
    }
    fn declared_result_type(&self, t: &JavaType) -> Result<Ty> {
        if self.source_owner(t).is_some() {
            Ok(Ty::Ref)
        } else {
            self.java_type(t)
        }
    }
}
impl Loaded {
    /// Closed source entrance: consume the exact AST validated by the source
    /// capability gate. No YAML roundtrip, native stand-in or general executor.
    pub fn from_linked_source(program: JavaProgram, root: &Path, locator: &str) -> Result<Self> {
        let resolved = source_entry::SourceEntry::resolve(&program, root, locator)?;
        let target = resolved.target.clone();
        Self::admit_source(program, &target, Some(resolved))
    }

    pub fn from_parsed_source(program: JavaProgram, entry: &str) -> Result<Self> {
        Self::admit_source(program, entry, None)
    }

    fn admit_source(program: JavaProgram, entry: &str, source_entry: Option<source_entry::SourceEntry>) -> Result<Self> {
        let receiver = source_entry.as_ref().and_then(|source| source.receiver.as_ref())
            .map(|receiver| (receiver.owner.as_str(), &receiver.assembly_fields));
        let crate::normalized::SourceEvidence {
            layouts: source_layouts,
            exception_parents: source_exception_parents,
            exception_bindings: source_exception_bindings,
            ports: source_ports,
            nullable_entry_parameters: source_nullable_entry_parameters,
        } = crate::normalized::validate_with_receiver(&program, entry, receiver)?;
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
            source_exception_bindings,
            source_nullable_entry_parameters,
            schema: KERNEL_INPUT_SCHEMA.into(),
            source: "<parsed-java-source>".into(),
            source_text: String::new(),
            entries: vec![entry.into()],
            functions: BTreeMap::new(),
            heap_fields,
            exceptions,
            ports,
            result_value_types: BTreeMap::new(),
            parameter_value_types: BTreeMap::new(),
        };
        // The source gate already checked syntax, calls and cycles. Only
        // normalize(entry) executes code, with the actual entry arguments and
        // receiver; auditing a constructor as a stand-alone call has no meaning.
        Ok(Self { project, program, source_entry })
    }
    fn read_adapter_project(path: &Path) -> Result<Self> {
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
        // Function and port excerpts frequently reference the same source.
        // Validate them against one adjacent snapshot without retaining all
        // source files or bypassing path admission for each origin.
        let mut origin_path = None;
        let mut original = String::new();
        for origin in project
            .functions
            .values()
            .map(|f| &f.origin)
            .chain(project.ports.values().map(|p| &p.origin))
        {
            let path = local_path(root, &origin.path)?;
            if origin_path.as_ref() != Some(&path) {
                drop(std::mem::take(&mut original));
                original = read(&path)?;
                origin_path = Some(path);
            }
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
        Ok(Self { project, program, source_entry: None })
    }
    fn engine(&self, checked_ports: BTreeMap<String, actions::CheckedPort>) -> Engine<'_> {
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
            active: BTreeSet::new(),
            steps: 0,
            selected_entry: None,
            entry_context: None,
            state_roots: BTreeMap::new(),
            checked_ports,
            pending_completions: BTreeMap::new(),
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
            realtime_store: realtime_state::SymbolicStore::default(),
            realtime_effects: realtime_state::EffectSequence::default(),
            realtime_completion: realtime_state::Completion::Continue,
            realtime_throws: Vec::new(),
        }
    }
    fn prepare(&self) -> Result<Engine<'_>> {
        let mut e = self.engine(self.project.validate_state_metadata()?);
        let mut classes = BTreeSet::new();
        let mut adapter_signatures = BTreeMap::new();
        let mut aliases = BTreeMap::<&str, Ty>::new();
        // Resolve on first use rather than before walking declarations:
        // a method may refer to a carrier declared later in the project.
        let mut adapter_alias = |name: &str| -> Result<Option<Ty>> {
            let Some((key, declared)) = self.project.result_value_types.get_key_value(name) else {
                return Ok(None);
            };
            if let Some(value) = aliases.get(key.as_str()) {
                return Ok(Some(value.clone()));
            }
            let value = Ty::parse(declared)?;
            aliases.insert(key.as_str(), value.clone());
            Ok(Some(value))
        };
        for c in self.program.classes() {
            need(
                !simulator_parser::is_reserved_identifier(c.simple_name()),
                "reserved Java class name; rename during preprocessing",
            )?;
            if !self.project.direct_source {
                // Generated carriers do not have the source admission proof.
                need(classes.insert(c.full_name()), "duplicate class")?;
                need(
                    c.supertype().is_none()
                        && c.interfaces().is_empty()
                        && !c.is_interface()
                        && c.fields().is_empty()
                        && c.annotations().is_empty()
                        && c.imports().is_empty()
                        && !c.has_unbound_imports(),
                    "unsupported class shape",
                )?;
                if let Some(ty) = adapter_alias(&c.full_name())? {
                    need(!matches!(c.full_name().as_str(), "Object" | "String")
                        && identifier(&c.full_name()) && c.methods().is_empty()
                        && matches!(ty, Ty::Vector(_) | Ty::Record(_) | Ty::OrderedTextMap),
                        "result value alias requires a declared empty carrier class and composite state type")?;
                }
            }
            if c.is_interface() {
                continue;
            }
            for (index, m) in c.methods().iter().enumerate() {
                let id = if self.project.direct_source {
                    crate::normalized::method_key(c, index, m)
                } else {
                    format!("{}.{}", c.full_name(), m.name())
                };
                if !self.project.direct_source {
                    need(m.body().is_none(), "adapter signature carriers must not contain Java method bodies")?;
                    need(m.annotations().is_empty(), "unsupported method shape")?;
                    need(!e.methods.contains_key(&id), "duplicate/overloaded method")?;
                }
                let return_type = m.return_type();
                let alias = match return_type {
                    JavaType::Class(name) if !self.project.direct_source => adapter_alias(name)?,
                    _ => None,
                };
                let returns = alias.map_or_else(|| self.project.declared_result_type(return_type), Ok)?;
                let defaults = if self.project.direct_source {
                    m.params().iter().map(|(ty, _)| self.project.parameter_type(ty))
                        .collect::<Result<Vec<_>>>()?
                } else {
                    let (types, spellings): (Vec<_>, Vec<_>) = m.params().iter()
                        .map(|(ty, _)| adapter_parameter(ty)).collect::<Result<Vec<_>>>()?
                        .into_iter().unzip();
                    adapter_signatures.insert(id.clone(), format!("{})", spellings.join(",")));
                    types
                };
                e.methods.insert(id, AdmittedMethod { syntax: m, defaults, returns, binding: None });
            }
        }
        // Source admission already owns executable/native declaration
        // validity. Semantic bindings only exist on signature-only adapters.
        if self.project.direct_source {
            return Ok(e);
        }
        for name in self.project.result_value_types.keys() {
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
            let declared = e.methods.get_mut(id)
                .ok_or_else(|| fail(format!("binding has no Java declaration: {id}")))?;
            let m = declared.syntax;
            need(signature.ends_with(')'), "invalid signature")?;
            need(adapter_signatures.get(id).is_some_and(|admitted| admitted == signature),
                "Java parameter signature mismatch")?;
            need(
                f.params.len() == m.params().len()
                    && Ty::parse(&f.returns)? == declared.returns,
                "semantic return/arity mismatch",
            )?;
            // Matching the declaration in slot order also proves uniqueness.
            let mut parameters = Vec::with_capacity(f.params.len());
            for ((p, (_, java_name)), fallback) in f.params.iter().zip(m.params()).zip(&declared.defaults) {
                need(
                    identifier(&p.name),
                    "duplicate/invalid parameter",
                )?;
                need(&p.name == java_name, "binding parameters must match the Java declaration")?;
                let ty = Ty::parse(&p.typ)?;
                let override_type = self.project.parameter_override(id.rsplit('.').next().unwrap_or(id), &p.name)?;
                need(ty == *override_type.as_ref().unwrap_or(fallback), "semantic parameter type mismatch")?;
                parameters.push(ty);
            }
            need(
                declared.binding.replace(Rc::new(CheckedBinding { name, function: f, parameters })).is_none(),
                "duplicate binding",
            )?;
        }
        for method in e.methods.values() {
            need(method.binding.is_some(), "unbound bodyless method")?;
        }
        Ok(e)
    }
    fn normalize(&self, entry: &str) -> Result<ExecutionResult> {
        let adapter_prepare = (!self.project.direct_source).then(Instant::now);
        let mut e = self.prepare()?;
        // The adapter prepares every declaration, but executes only the
        // selected entry. Reuse this verified engine rather than rebuilding it.
        if let Some(started) = adapter_prepare {
            for id in &self.project.entries {
                need(e.methods.contains_key(id), "entry missing")?;
            }
            emit_stage_profile("prepare", started, format!("methods={}", e.methods.len()));
        }
        let selected = self.project.entries.iter().find(|s| *s == entry)
            .ok_or_else(|| fail("entry not declared"))?;
        let m = e.methods.get(entry).ok_or_else(|| fail("entry method missing"))?.syntax;
        e.selected_entry = Some(kernel_execution::SelectedEntry { id: selected.as_str(), syntax: m });
        e.entry_context = self.project.entry_contexts.get(entry);
        let args = m
            .params()
            .iter()
            .enumerate()
            .map(|(index, (t, n))| {
                let expected = e.methods[entry].parameter_types()[index].clone();
                let nullable = if self.project.direct_source {
                    self.project.source_nullable_entry_parameters.contains(n)
                } else {
                    let owner = entry.rsplit_once('.').ok_or_else(|| fail("missing entry owner"))?.0;
                    let class = e.program.find_class(owner).ok_or_else(|| fail("entry owner missing"))?;
                    crate::normalized::source_nullable_parameter(class, m, n, t)?
                };
                e.entry_argument(n, t, &expected, nullable)
            })
            .collect::<Result<_>>()?;
        let entry_started = Instant::now();
        let value = if self.source_entry.as_ref().is_some_and(|source| source.receiver.is_some()) {
            e.java_call_on_certified_entry_receiver(entry, args, b::lit_bool(true))?
        } else {
            e.java_call(entry, args, b::lit_bool(true))?
        };
        e.emit_stage_profile(
            "entry_execute",
            entry_started,
            "completed_entry_execute",
        );
        let source_record_return = if e.realtime_completion.is_always_throwing() {
            None
        } else {
            e.source_record(m.return_type(), &value)?
        };
        let realtime_completion = e.realtime_completion.replace_continue(
            realtime_state::Completion::Return(value.scalar()?),
        );
        let terminal = realtime_completion.terminal_alternatives()
            .ok_or_else(|| fail("entry has incomplete control flow"))?;
        e.emit_profile_metrics("completed");
        Ok(ExecutionResult {
            temporal_definitions: e.temporal_definitions,
            source_state_bindings: e.source_objects.state_bindings,
            source_identity_parameters: e.source_objects.compared_parameters,
            source_identity_states: e.source_objects.compared_states,
            source_record_return,
            entry_return_type: e.methods[entry].returns.clone(),
            source_tables: e.source_relations.tables,
            realtime_effects: e.realtime_effects,
            terminal,
        })
    }
}
mod pure_fold;
mod member_choice;
mod temporal_graph;
mod business_entry;
mod source_entry;
pub(crate) use source_entry::inert_singleton;
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
fn java_carrier_atom(t: &JavaType) -> Option<(Ty, &str)> {
    match t {
        JavaType::Primitive(name) if crate::normalized::integral(t) => Some((Ty::Integer, name)),
        JavaType::Primitive(name) if name == "boolean" => Some((Ty::Bool, name)),
        JavaType::Class(name) if name == "Object" || name == "java.lang.Object" => Some((Ty::Ref, name)),
        JavaType::Class(name) if name == "String" => Some((Ty::Text, name)),
        _ => None,
    }
}

fn adapter_integer_component(t: &JavaType) -> Option<&str> {
    match t {
        // The adapter's exact semantic override still supplies vector<number>.
        JavaType::Primitive(name) if matches!(name.as_str(), "int" | "long") => Some(name),
        _ => None,
    }
}

fn adapter_parameter(t: &JavaType) -> Result<(Ty, String)> {
    if let Some((ty, spelling)) = java_carrier_atom(t) {
        return Ok((ty, spelling.to_owned()));
    }
    if let JavaType::Array(item) = t {
        let spelling = adapter_integer_component(item)
            .ok_or_else(|| fail("unsupported Java array type"))?;
        return Ok((Ty::Seq(Box::new(Ty::Integer)), format!("{spelling}[]")));
    }
    Err(fail("unsupported Java type"))
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
    let (record, _, _) = math::domains::composite::field_projection(expr)?;
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
        Ty::Vector(_) | Ty::Record(_) => Value::Term(
            t.clone(),
            math::domains::composite::input(name),
        ),
        Ty::Seq(item) => Value::Sequence {
            item: *item.clone(),
            java_component: declared.and_then(primitive_array_component).map(str::to_owned),
            name: name.into(),
            null: b::eq(
                b::heap_ref(heap.new_path_object().id()),
                b::lit_null(),
            ),
        },
        Ty::Ref => Value::Term(
            Ty::Ref,
            b::heap_ref(heap.new_path_object().id()),
        ),
        Ty::Bool => Value::Term(t.clone(), b::param(name, ScalarSymbolType::Boolean)),
        Ty::Integer => Value::Term(t.clone(), b::param(name, ScalarSymbolType::Integer)),
        Ty::Decimal => Value::Term(t.clone(), b::param(name, ScalarSymbolType::ExactDecimal)),
        Ty::Text => Value::Term(t.clone(), b::param(name, ScalarSymbolType::Text)),
        Ty::Unit | Ty::Task(_) | Ty::OrderedTextMap | Ty::LocalTextMap | Ty::LocalRelation =>
            return Err(fail("this value type has no input symbol contract")),
    })
}
type Env = BTreeMap<String, Value>;

fn select(c: Expr, yes: Value, no: Value) -> Result<Value> {
    let then_ty = yes.ty();
    let else_ty = no.ty();
    need(then_ty == else_ty, "branch result type mismatch")?;
    let (yes, no) = match (yes, no) {
        (Value::LocalTextMap(yes), Value::LocalTextMap(no)) =>
            return Ok(Value::LocalTextMap(yes.join(c, no))),
        other => other,
    };
    let ty = then_ty;
    if ty == Ty::Integer {
        if let Some(extremum) = math::case_orientation::ordered_number_selection(
            &c, &yes.scalar()?, &no.scalar()?, &math::DefaultSortContext,
        ) {
            return Ok(Value::Term(ty, math::normalize(extremum)));
        }
    }
    let value = realtime_state::join_value(c, yes.scalar()?, no.scalar()?);
    Ok(Value::Term(ty, value))
}

#[cfg(test)]
mod typed_carrier_contract_tests {
    use super::*;

    fn empty_adapter() -> Project {
        serde_json::from_value(serde_json::json!({
            "schema": KERNEL_INPUT_SCHEMA, "source": "Carrier.java",
            "source_text": "class Carrier {}", "entries": [], "functions": {}
        })).unwrap()
    }

    fn array(item: JavaType) -> JavaType {
        JavaType::Array(Box::new(item))
    }

    #[test]
    fn source_array_proof_is_not_an_adapter_result_alias() {
        let integer = JavaType::Primitive("int".into());
        let row = JavaType::Class("Row".into());
        let adapter = empty_adapter();
        for carrier in [array(integer.clone()), array(array(integer.clone())), array(row.clone())] {
            assert!(adapter.declared_result_type(&carrier).unwrap_err().contains("unsupported Java array type"));
        }

        let mut source = empty_adapter();
        source.direct_source = true;
        source.source_layouts.insert("Row".into(), BTreeMap::new());
        assert_eq!(source.declared_result_type(&array(integer.clone())).unwrap(),
            Ty::Seq(Box::new(Ty::Integer)));
        assert_eq!(source.declared_result_type(&array(array(integer))).unwrap(),
            Ty::Seq(Box::new(Ty::Seq(Box::new(Ty::Integer)))));
        assert_eq!(source.declared_result_type(&array(row)).unwrap(), Ty::Seq(Box::new(Ty::Ref)));

        // Even the direct source route needs the matching component proof.
        for carrier in [
            array(array(JavaType::Primitive("boolean".into()))),
            array(JavaType::Class("Other".into())),
        ] {
            assert!(source.declared_result_type(&carrier).unwrap_err().contains("unsupported Java array type"));
        }
    }

    #[test]
    fn wire_identifiers_need_a_nonempty_identifier_start() {
        for spelling in ["", "7port"] {
            assert!(!identifier(spelling), "{spelling:?}");
        }
        assert!(identifier("port7"));
    }

    #[test]
    fn acquired_floor_needs_a_live_path_that_implies_its_domain() {
        let available = b::param("available", math::ScalarSymbolType::Boolean);
        let acquired = Value::IntegerFloor {
            value: b::param("count", math::ScalarSymbolType::Integer),
            lower: 0.into(),
            domain: available.clone(),
        };
        assert_eq!(acquired.certified_integer_floor(&available), Some(0.into()));
        assert_eq!(acquired.certified_integer_floor(&b::lit_bool(false)), None);
        assert_eq!(acquired.certified_integer_floor(&b::not(available)), None);
        assert_eq!(acquired.certified_integer_floor(&b::lit_bool(true)), None);
    }

    #[test]
    fn public_temporal_projection_preserves_the_difference_between_record_and_integer() {
        let record = b::record_temporal_result(7);
        let integer = b::integer_temporal_result(11);
        assert!(is_record_choice(&record));
        assert!(!is_record_choice(&integer));
        assert_eq!(integer_definition_id(&integer), Some(11));
        assert_eq!(integer_definition_id(&record), None);
    }
}
