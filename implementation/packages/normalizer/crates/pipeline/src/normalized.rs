//! First, deliberately small normalized input profile. No plugin discovery.
//! This structural gate is not a Java type checker or a translation proof.
mod external_polling;
mod static_types;
pub(crate) mod fixed_for;
pub(crate) mod generic_product;
pub(crate) mod affine_recursion;
pub(crate) mod source_ports;

use simulator_parser::{
    AnnotationRef, JavaBinOp as B, JavaExpr as E, JavaClass, JavaLit, JavaMethod, JavaProgram, JavaStatement as S, JavaType,
    JavaUnaryOp as U,
};
use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
};

fn error(code: &str, at: &str) -> String {
    let action = match code {
        "NORM-LOOP" => "按 Skill 将循环改写为固定遍历语义并保留状态、调用顺序及异常；Rust 尚未接入 fold_until，请报告能力缺口，不要伪造无循环实现。",
        "NORM-RECURSION" => "按 Skill 消解递归调用环；无法用已有固定语义表达时报告能力缺口，不要截断递归深度。",
        "NORM-UNBOUND-CALL" | "NORM-MISSING-BODY" => "按 Skill 提供闭合函数体或真实外部交互依据；不得用空实现、默认值或伪造输入隐藏缺口。",
        _ => "按 Skill 对照当前 scalar-0 能力准备规范 Java；超出当前能力的类型、状态或语义绑定应报告缺口，不得删除业务行为。",
    };
    crate::preprocess::required(code, at, "输入不满足当前规范 Java 合同。", action)
}
// Only a pure literal constructor initializer is admitted here. The same
// typed checker is used by layout admission and the object allocator.
pub(crate) fn closed_field_initializer(t: &JavaType, e: &E) -> Option<math::Expr> {
    use math::builder as b;
    fn integer(e: &E) -> Option<(i128, bool)> {
        match e {
            E::Lit(JavaLit::Int(value)) => Some((i128::from(*value), false)),
            E::Lit(JavaLit::Long(value)) => Some((i128::from(*value), true)),
            E::UnaryOp { op: U::Neg, operand } =>
                integer(operand).map(|(value, long)| (-value, long)),
            _ => None,
        }
    }
    match (t, e) {
        (JavaType::Primitive(ty), e) if integral(t) => {
            let (value, long) = integer(e)?;
            let fits = match ty.as_str() {
                "byte" => !long && i8::try_from(value).is_ok(),
                "short" => !long && i16::try_from(value).is_ok(),
                "int" => !long && i32::try_from(value).is_ok(),
                "long" => i64::try_from(value).is_ok(),
                _ => false,
            };
            fits.then(|| b::lit_integer(value))
        }
        (JavaType::Primitive(ty), E::Lit(JavaLit::Bool(value))) if ty == "boolean" => Some(b::lit_bool(*value)),
        (JavaType::Class(ty), E::Lit(JavaLit::Str(value))) if matches!(ty.as_str(), "String" | "java.lang.String") => Some(b::lit_str(value)),
        (JavaType::Class(_), E::Lit(JavaLit::Null)) => Some(b::lit_null()),
        _ => None,
    }
}

/// A well-typed host-only declaration may exist in an uncalled method. The
/// type relation here is declaration evidence, not a library implementation;
/// actual execution of the allocation must still fail closed.
pub(crate) fn host_collection_allocation(declared: &JavaType, init: &E) -> bool {
    let JavaType::Generic(interface, element) = declared else { return false; };
    if interface != "java.util.List" || element.len() != 1
        || !matches!(&element[0], JavaType::Class(name) if matches!(name.as_str(), "java.lang.String" | "String"))
    { return false; }
    matches!(init, E::New { java_type: JavaType::Generic(allocator, diamond), args }
        if allocator == "java.util.ArrayList" && diamond.is_empty() && args.is_empty())
}

pub(crate) fn integral(t: &JavaType) -> bool {
    matches!(t, JavaType::Primitive(n) if matches!(n.as_str(), "byte" | "short" | "int" | "long"))
}
pub(crate) fn native_decimal_type(ty: &JavaType) -> bool {
    matches!(ty, JavaType::Primitive(name) if matches!(name.as_str(), "float" | "double"))
}
pub(crate) fn exact_decimal_type(ty: &JavaType) -> bool {
    // Fully qualified reserved carrier, with exact decimal value semantics
    // only for this profile. java.lang.Double / native double stay separate.
    matches!(ty, JavaType::Class(name) if name == "java.math.BigDecimal")
}

pub(crate) fn calendar_date_type(ty: &JavaType) -> bool {
    matches!(ty, JavaType::Class(name) if name == "java.time.LocalDate")
}

pub(crate) fn numeric_array(ty: &JavaType) -> bool {
    matches!(ty, JavaType::Array(item) if integral(item))
}

fn record_array_component(ty: &JavaType) -> Option<&JavaType> {
    match ty {
        JavaType::Array(component) if matches!(component.as_ref(), JavaType::Class(_)) => Some(component),
        _ => None,
    }
}

fn scalar(t: &JavaType) -> bool {
    integral(t)
        || matches!(t, JavaType::Primitive(n) if n == "boolean")
        || matches!(t, JavaType::Class(n) if matches!(n.as_str(), "Object" | "java.lang.Object" | "String" | "java.lang.String"))
}

pub(crate) fn plain_source_interface(class: &simulator_parser::JavaClass) -> bool {
    class.is_interface()
        && class.fields().is_empty()
        && class.interfaces().is_empty()
        && !class.methods().is_empty()
        && class.methods().iter().all(|method| !method.is_static()
            && method.body().is_some() == method.is_default() && method.throws().is_empty()
            && method.annotations().is_empty() && method.type_parameters().is_empty()
            && scalar(method.return_type()) && method.params().iter().all(|(ty, _)| scalar(ty)))
}

pub(crate) fn override_matches(interface: &simulator_parser::JavaMethod, concrete: &simulator_parser::JavaMethod) -> bool {
    interface.name() == concrete.name() && !concrete.is_static() && concrete.is_public() && concrete.body().is_some()
        && interface.return_type() == concrete.return_type()
        && interface.params().iter().map(|(ty, _)| ty).eq(concrete.params().iter().map(|(ty, _)| ty))
}

pub(crate) fn source_record_accessor<'a>(
    class: &'a JavaClass, member: &str, arity: usize, selected_content: bool,
) -> Option<&'a JavaType> {
    if arity != 0 { return None; }
    let field = class.fields().iter().find(|field| !field.is_static() && field.name() == member)?;
    if class.is_record() { return Some(field.java_type()); }
    if !selected_content { return None; }
    let method = class.methods().iter().find(|m| m.name() == member && m.arity() == 0
        && !m.is_static() && m.return_type() == field.java_type())?;
    let [S::Return(Some(E::Ident(returned)))] = method.body()? else { return None; };
    (returned == field.name()).then(|| field.java_type())
}

fn source_record_local(program: &JavaProgram, method: &JavaMethod, local: &str, member: &str, arity: usize) -> bool {
    let declared = method.params().iter().find(|(_, name)| name == local).map(|(ty, _)| ty)
        .or_else(|| method.body()?.iter().find_map(|statement| match statement {
            S::VarDecl { java_type, name, .. } if name == local => Some(java_type),
            _ => None,
        }));
    let Some(JavaType::Class(owner)) = declared else { return false; };
    let selected = method.body().is_some_and(|body| body.iter().any(|statement|
        matches!(statement, S::LocalIr(normalizer_ir::local::LocalIr::MemberChoice { binding, .. }) if binding == local)));
    program.find_class(owner).is_some_and(|class|
        source_record_accessor(class, member, arity, selected).is_some())
}

fn unambiguous_interface_defaults(program: &JavaProgram, class: &JavaClass) -> bool {
    let defaults = class.interfaces().iter().filter_map(|name| program.find_class(name))
        .flat_map(|interface| interface.methods().iter()).filter(|member| member.is_default())
        .filter(|member| !class.methods().iter().any(|method| override_matches(member, method)))
        .collect::<Vec<_>>();
    defaults.iter().enumerate().all(|(index, method)| !defaults[..index].iter()
        .any(|other| other.name() == method.name() && same_parameters(other, method)))
}

pub(crate) fn specialized_interface<'a>(program: &'a JavaProgram, ty: &JavaType) -> Option<&'a JavaClass> {
    let (name, args) = match ty {
        JavaType::Class(name) => (name, &[][..]),
        JavaType::Generic(name, args) => (name, args.as_slice()),
        _ => return None,
    };
    let class = program.find_class(name).filter(|class| plain_source_interface(class))?;
    (class.type_parameters().len() == args.len() && args.iter().all(|arg|
        matches!(arg, JavaType::Class(name) if program.find_class(name).is_some()
            || matches!(name.as_str(), "String" | "java.lang.String" | "Object" | "java.lang.Object"
                | "Integer" | "java.lang.Integer" | "Long" | "java.lang.Long" | "Boolean" | "java.lang.Boolean"))))
        .then_some(class)
}

pub(crate) fn implements_specialization(class: &JavaClass, ty: &JavaType) -> bool {
    let (name, args) = match ty {
        JavaType::Class(name) => (name, &[][..]),
        JavaType::Generic(name, args) => (name, args.as_slice()),
        _ => return false,
    };
    class.interfaces().contains(name) && class.interface_arguments(name) == args
}

fn local_interface_receiver<'a>(
    program: &'a JavaProgram, method: &simulator_parser::JavaMethod, local: &str,
) -> Option<&'a simulator_parser::JavaClass> {
    let declared = method.params().iter().find(|(_, name)| name == local).map(|(ty, _)| ty)
        .or_else(|| method.body()?.iter().find_map(|statement| match statement {
            S::VarDecl { java_type, name, .. } if name == local => Some(java_type),
            _ => None,
        }))?;
    specialized_interface(program, declared)
}

fn builtin_resource_interface(program: &JavaProgram, interface: &str) -> bool {
    interface == "java.lang.AutoCloseable"
        || (interface == "AutoCloseable" && program.find_class(interface).is_none())
}

fn closed_resource_close(program: &JavaProgram, class: &JavaClass) -> Option<String> {
    if !class.interfaces().iter().any(|interface| builtin_resource_interface(program, interface)) {
        return None;
    }
    let (index, method) = class.methods().iter().enumerate()
        .find(|(_, method)| method.name() == "close" && method.arity() == 0)?;
    if !method.is_public() || method.is_static() || !method.throws().is_empty()
        || *method.return_type() != JavaType::Void || method.body().is_none() {
        return None;
    }
    Some(method_key(class, index, method))
}

fn resource_blank_final(class: &JavaClass, field: &simulator_parser::ast::JavaField) -> bool {
    if field.has_initializer() { return false; }
    let mut all = class.methods().iter().filter(|method| method.is_constructor());
    let Some(constructor) = all.next() else { return false };
    if all.next().is_some() { return false; }
    // A single simple constructor must definitely initialize this field.
    // The static checker also rejects later writes to the final field.
    constructor.body().is_some_and(|body| {
        let mut count = 0;
        for stmt in body {
            let S::Assign { target: E::FieldAccess { target, field: assigned }, value: E::Ident(parameter),
                is_compound: false } = stmt else { return false; };
            if !matches!(target.as_ref(), E::Ident(name) if name == "this")
                || !constructor.params().iter().any(|(ty, name)| name == parameter
                    && class.fields().iter().any(|f| f.name() == assigned && f.java_type() == ty)) {
                return false;
            }
            count += usize::from(assigned == field.name());
        }
        count == 1
    })
}

pub(crate) fn closed_resource_binding<'a>(
    program: &JavaProgram, resource: &'a S,
) -> Option<(&'a str, String)> {
    let S::VarDecl { name, java_type: JavaType::Class(owner), initializer:
        Some(E::New { java_type: JavaType::Class(allocation), .. }), .. } = resource else { return None; };
    if owner != allocation { return None; }
    let class = program.find_class(owner)?;
    Some((name, closed_resource_close(program, class)?))
}

/// Source layouts retain declaring owners. Primitive DTO fields follow the
/// business numeric domain and the existing Object carrier remains opaque.
pub(crate) fn source_layouts(
    program: &JavaProgram,
    receiver: Option<(&str, &BTreeSet<String>)>,
) -> Result<BTreeMap<String, BTreeMap<String, String>>, String> {
    let mut layouts = BTreeMap::new();
    for class in program.classes() {
        if !class.static_initializers().is_empty() && !receiver.is_some_and(|(owner, _)| owner == class.full_name()) {
            return Err(error("NORM-STATIC-INITIALIZER", &class.full_name()));
        }
        if source_payload_domain(class).is_some() || generic_product::prove(class).is_some() { continue; }
        if class.fields().is_empty() {
            if class.supertype().is_none()
                && (!class.is_interface() || plain_source_interface(class)) {
                layouts.insert(class.full_name(), BTreeMap::new());
            }
            continue;
        }
        let mut fields = BTreeMap::new();
        let mut declared_field_names = BTreeSet::new();
        for field in class.fields() {
            if !declared_field_names.insert(field.name()) {
                return Err(error("NORM-FIELD-DECLARATION", &class.full_name()));
            }
            if receiver.is_some_and(|(owner, fields)| owner == class.full_name() && fields.contains(field.name())) {
                continue;
            }
            if crate::kernel_input::inert_singleton(program, class, field) {
                continue;
            }
            if source_ports::marked(class, field) {
                continue;
            }
            let state = entry_state(class, field);
            let annotation_ok = field.annotations().is_empty() || state || source_nullable_field(class, field);
            let initializer_ok = field.initializer().is_none_or(|value| {
                if state {
                    // This only certifies assembly before the selected entry.
                    // Execution still reads the acquired E snapshot, not the
                    // values produced by this original constructor.
                    crate::kernel_input::inert_data_allocation(program, field.java_type(), value)
                } else {
                    closed_field_initializer(field.java_type(), value).is_some()
                }
            });
            if field.is_static()
                || field.is_final() && !class.is_record()
                    && !(closed_resource_close(program, class).is_some() && resource_blank_final(class, field))
                || !initializer_ok || !annotation_ok
            {
                return Err(error("NORM-FIELD-DECLARATION", &class.full_name()));
            }
            let ty = match field.java_type() {
                t if integral(t) => "number",
                t if entry_state(class, field) && numeric_array(t) => "seq<number>",
                JavaType::Primitive(n) if n == "boolean" => "bool",
                JavaType::Class(n) if n == "String" || n == "java.lang.String" => "text?",
                JavaType::Class(n)
                    if n == "Object"
                        || n == "java.lang.Object"
                        || program.find_class(n).is_some() =>
                {
                    "ref?"
                }
                _ => return Err(error("NORM-FIELD-DECLARATION", &class.full_name())),
            };
            // Every field name was checked before any of the admission paths above.
            fields.insert(field.name().into(), ty.into());
        }
        layouts.insert(class.full_name(), fields);
    }
    // Lexical short names are accepted only with one actual declared owner.
    let mut aliases = BTreeSet::new();
    for owner in layouts.keys() {
        let short = owner.rsplit('.').next().unwrap();
        if !aliases.insert(short.to_string()) {
            return Err(error("NORM-AMBIGUOUS-OWNER", short));
        }
    }
    Ok(layouts)
}
const ENTRY_STATE: &str = "normalizer.annotations.EntryState";
const NULLABLE: &str = "normalizer.annotations.Nullable";
pub(crate) const N_IMPORT: &str = "normalizer.intrinsics.N";

fn official_member(marker: &str) -> Option<(&'static str, &'static str)> {
    match marker {
        ENTRY_STATE => Some(("N.EntryState", "normalizer.intrinsics.N.EntryState")),
        NULLABLE => Some(("N.Nullable", "normalizer.intrinsics.N.Nullable")),
        source_ports::IMPORT => Some(("N.External", "normalizer.intrinsics.N.External")),
        _ => None,
    }
}

pub(crate) fn imports_official_n(class: &simulator_parser::JavaClass) -> bool {
    class.imports().iter().any(|name| name == N_IMPORT)
        && class.imports().iter().all(|name| name == N_IMPORT
            || name.rsplit('.').next() != Some("N"))
        && !class.has_unbound_imports()
}

/// The closed Java profile admits no package declarations. Refuse an ambiguous
/// local type rather than treating its nested markers as Normalizer markers.
fn validate_official_namespace(program: &JavaProgram) -> Result<(), String> {
    let declared_n = program.classes().iter()
        .any(|class| class.simple_name().rsplit('.').next() == Some("N"));
    let counterfeit_namespace = program.classes().iter().any(|class| {
        let name = class.simple_name();
        name == N_IMPORT || name.strip_prefix(N_IMPORT).is_some_and(|tail| tail.starts_with('.'))
    });
    if counterfeit_namespace || declared_n && program.classes().iter().any(imports_official_n) {
        return Err(error("NORM-INTRINSIC-SHADOW", N_IMPORT));
    }
    for (qualified, alias) in [("java.math.BigDecimal", "BigDecimal"), ("java.time.LocalDate", "LocalDate")] {
        if program.classes().iter().any(|class| class.simple_name() == qualified)
            || (program.classes().iter().any(|class| class.simple_name().rsplit('.').next() == Some(alias))
                && program.classes().iter().any(|class| class.imports().iter().any(|name| name == qualified)))
        {
            return Err(error("NORM-INTRINSIC-SHADOW", qualified));
        }
    }
    Ok(())
}

fn official_marker(class: &simulator_parser::JavaClass, annotations: &[AnnotationRef], qualified: &str) -> bool {
    let [annotation] = annotations else { return false; };
    !annotation.has_arguments() && official_member(qualified).is_some_and(|(short, full)| {
        annotation.name() == full || (annotation.name() == short && imports_official_n(class))
    })
}

fn source_marker(
    class: &simulator_parser::JavaClass,
    annotations: &[AnnotationRef],
    qualified: &str,
) -> bool {
    let [annotation] = annotations else { return false; };
    if annotation.has_arguments() {
        return false;
    }
    if official_marker(class, annotations, qualified) {
        return true;
    }
    // Continue decoding explicitly bound legacy annotations in existing
    // adapters; an application marker with only a matching suffix is not one.
    annotation.name() == qualified
        || (qualified.rsplit('.').next() == Some(annotation.name())
            && class.imports().iter().any(|name| name == qualified))
}

pub(crate) fn entry_state(
    class: &simulator_parser::JavaClass,
    field: &simulator_parser::JavaField,
) -> bool {
    source_marker(class, field.annotations(), ENTRY_STATE)
}
pub(crate) fn source_nullable_parameter(
    class: &simulator_parser::JavaClass, method: &simulator_parser::JavaMethod, name: &str, ty: &JavaType,
) -> Result<bool, String> {
    match method.param_annotations(name) {
        [] => Ok(false),
        annotations @ [_] if matches!(ty, JavaType::Class(_))
            && source_marker(class, annotations, NULLABLE) => Ok(true),
        [annotation] => Err(format!("unsupported source parameter annotation {}", annotation.name())),
        _ => Err("multiple source parameter annotations cannot define nullability".into()),
    }
}

pub(crate) fn source_nullable_field(
    class: &simulator_parser::JavaClass, field: &simulator_parser::JavaField,
) -> bool {
    matches!(field.java_type(), JavaType::Class(_))
        && source_marker(class, field.annotations(), NULLABLE)
}

pub(crate) fn nullable_text_field(
    class: &simulator_parser::JavaClass, field: &simulator_parser::JavaField,
) -> bool {
    matches!(field.java_type(), JavaType::Class(name) if name == "String" || name == "java.lang.String")
        && source_nullable_field(class, field)
}
pub(crate) fn nullable_text_return(class: &simulator_parser::JavaClass, method: &simulator_parser::JavaMethod) -> bool {
    matches!(method.return_type(), JavaType::Class(name) if name == "String" || name == "java.lang.String")
        && source_marker(class, method.annotations(), NULLABLE)
}

pub(crate) fn supported_source_imports(class: &simulator_parser::JavaClass) -> bool {
    class.imports().iter().all(|name| {
        matches!(
            name.as_str(),
            source_ports::IMPORT | ENTRY_STATE | NULLABLE | N_IMPORT | "java.util.List" | "java.math.BigDecimal" | "java.time.LocalDate" | crate::normalized_text::SOURCE_IMPORT
        )
    })
}
fn declared_reference(t: &JavaType, layouts: &BTreeMap<String, BTreeMap<String, String>>) -> bool {
    matches!(t, JavaType::Class(name) if layouts.keys().any(|owner| owner == name || owner.rsplit('.').next() == Some(name.as_str())))
}

pub(crate) const RUNTIME_EXCEPTION: &str = "java.lang.RuntimeException";
// The direct source profile models only this closed subset of the Java
// exception hierarchy. Qualified names are also the published identities.
const SOURCE_BUILTIN_EXCEPTIONS: &[(&str, Option<&str>)] = &[
    ("java.lang.Throwable", None),
    ("java.lang.Exception", Some("java.lang.Throwable")),
    (RUNTIME_EXCEPTION, Some("java.lang.Exception")),
    ("java.lang.NullPointerException", Some(RUNTIME_EXCEPTION)),
    ("java.lang.ArithmeticException", Some(RUNTIME_EXCEPTION)),
    ("java.lang.IndexOutOfBoundsException", Some(RUNTIME_EXCEPTION)),
    ("java.lang.ArrayIndexOutOfBoundsException", Some("java.lang.IndexOutOfBoundsException")),
    ("java.lang.NegativeArraySizeException", Some(RUNTIME_EXCEPTION)),
];

fn builtin_exception(name: &str) -> Option<(&'static str, Option<&'static str>)> {
    SOURCE_BUILTIN_EXCEPTIONS.iter().copied().find(|(qualified, _)| {
        *qualified == name || qualified.strip_prefix("java.lang.") == Some(name)
    })
}

pub(crate) fn builtin_exception_name(name: &str) -> Option<&'static str> {
    builtin_exception(name).map(|(qualified, _)| qualified)
}

pub(crate) fn exception_name(name: &str, owner: &str, parents: &BTreeMap<String, String>) -> Option<String> {
    if let Some(canonical) = builtin_exception_name(name) {
        return Some(canonical.into());
    }
    if parents.contains_key(name) {
        return Some(name.into());
    }
    let lexical = format!("{owner}.{name}");
    parents.contains_key(&lexical).then_some(lexical)
}
/// Closed data carried by the external exception, not a Java heap target.
pub(crate) fn source_payload_domain(class: &simulator_parser::JavaClass) -> Option<&'static str> {
    if !matches!(class.supertype(), Some("RuntimeException" | RUNTIME_EXCEPTION)) { return None; }
    let [field] = class.fields() else { return None; };
    let field = Some(field).filter(|field| field.name() == "payload" && !field.is_static()
        && !field.is_final() && !field.has_initializer())?;
    match field.java_type() {
        JavaType::Class(name) if matches!(name.as_str(), "String" | "java.lang.String")
            && official_marker(class, field.annotations(), NULLABLE) => Some("text?"),
        ty if integral(ty) && field.annotations().is_empty() => Some("number"),
        _ => None,
    }
}

pub(crate) fn source_exceptions(program: &JavaProgram) -> Result<BTreeMap<String, String>, String> {
    let mut parents = BTreeMap::new();
    for class in program.classes() {
        // The tiny builtin hierarchy is authoritative only when source cannot
        // shadow one of its names. Reject rather than guess lexical rebinding.
        if builtin_exception_name(class.simple_name().rsplit('.').next().unwrap()).is_some() {
            return Err(error("NORM-EXCEPTION-SHADOW", &class.full_name()));
        }
        if let Some(parent) = class.supertype() {
            if (!class.fields().is_empty() && source_payload_domain(class).is_none())
                || !class.methods().is_empty()
                || !class.annotations().is_empty()
                || !class.interfaces().is_empty()
            {
                return Err(error("NORM-EXCEPTION-DECLARATION", &class.full_name()));
            }
            parents.insert(class.full_name(), parent.to_string());
        }
    }
    let snapshot = parents.clone();
    for (name, parent) in &mut parents {
        let owner = name.rsplit_once('.').map(|(owner, _)| owner).unwrap_or("");
        *parent = exception_name(parent, owner, &snapshot)
            .ok_or_else(|| error("NORM-EXCEPTION-BASE", name))?;
    }
    for name in parents.keys() {
        let mut current = name.as_str();
        let mut seen = BTreeSet::new();
        while current != "java.lang.Throwable" {
            if !seen.insert(current) {
                return Err(error("NORM-EXCEPTION-CYCLE", name));
            }
            current = parents
                .get(current).map(String::as_str)
                .or_else(|| builtin_exception(current).and_then(|(_, parent)| parent))
                .ok_or_else(|| error("NORM-EXCEPTION-BASE", name))?;
        }
    }
    Ok(parents)
}
pub(crate) fn subtype(child: &str, parent: &str, parents: &BTreeMap<String, String>) -> bool {
    let mut current = child;
    loop {
        if current == parent {
            return true;
        }
        let Some(next) = parents.get(current).map(String::as_str)
            .or_else(|| builtin_exception(current).and_then(|(_, parent)| parent))
        else {
            return false;
        };
        current = next;
    }
}
/// A protected expression may bind local temporaries in evaluation order. An
/// exception can leave at any binding. Such temporaries are out of scope in
/// every handler, so their partial values need no exceptional snapshot.
pub(crate) fn protected_return(body: &[S]) -> bool {
    fn isolated(statement: &S) -> bool {
        match statement {
            S::VarDecl { initializer: Some(_), .. } | S::Throw(_) | S::Return(_) => true,
            S::Block(body) => body.iter().all(isolated),
            S::If { then_block, else_block, .. } => then_block.iter().all(isolated)
                && else_block.as_ref().is_none_or(|other| other.iter().all(isolated)),
            _ => false,
        }
    }
    matches!(body.split_last(), Some((S::Return(_), prefix)) if prefix.iter().all(isolated))
}

/// Syntactic terminal-throw shape. Admission binds its declared type before execution.
pub(crate) fn finally_throw<'a>(statement: &'a S) -> Option<&'a str> {
    match statement {
        S::Throw(E::New { java_type: JavaType::Class(class), args }) if args.is_empty() => Some(class),
        _ => None,
    }
}

// A continuing protected block may change heap cells, but never rebind an
// outer local: guarded stores are shared across normal and caught paths.
fn continuing_try_body(body: &[S]) -> bool {
    body.iter().all(|statement| match statement {
        S::Assign { target: E::ArrayAccess { .. }, .. } | S::Throw(_) => true,
        S::If { then_block, else_block, .. } => continuing_try_body(then_block)
            && else_block.as_deref().is_none_or(continuing_try_body),
        _ => false,
    })
}
fn validate_protected_try(body: &[S], catches: &[simulator_parser::ast::CatchClause], finally: Option<&[S]>, id: &str) -> Result<(), String> {
    let closed_handlers = catches.iter().all(|handler|
        protected_return(handler.body()) || finally.is_none() && handler.body().is_empty());
    let terminal = protected_return(body);
    let continuing = catches.len() == 1 && finally.is_none() && continuing_try_body(body);
    if closed_handlers && !catches.is_empty() && (terminal || continuing) {
        return Ok(());
    }
    Err(error("NORM-CATCH-BODY", id))
}

pub(crate) struct SourceEvidence {
    pub(crate) layouts: BTreeMap<String, BTreeMap<String, String>>,
    pub(crate) exception_parents: BTreeMap<String, String>,
    pub(crate) exception_bindings: ExceptionBindings,
    pub(crate) ports: BTreeMap<String, source_ports::SourcePort>,
    pub(crate) nullable_entry_parameters: BTreeSet<String>,
}

// Keep declaration keys separate from Java call spellings and public locators.
// The source index only disambiguates an overload set inside this parsed class.
pub(crate) fn method_key(class: &JavaClass, index: usize, method: &JavaMethod) -> String {
    let base = format!("{}.{}", class.full_name(), method.name());
    if class.methods().iter().filter(|m| m.name() == method.name()).count() > 1 {
        format!("{base}@{index}")
    } else {
        base
    }
}

pub(crate) fn method_for_key<'a>(class: &'a JavaClass, key: &str) -> Option<&'a JavaMethod> {
    class.methods().iter().enumerate().find_map(|(index, method)|
        (method_key(class, index, method) == key).then_some(method))
}

pub(crate) fn constructor_for_arity(class: &JavaClass, arity: usize) -> Option<(String, &JavaMethod)> {
    let mut matches = class.methods().iter().enumerate()
        .filter(|(_, method)| method.is_constructor() && method.arity() == arity);
    let (index, method) = matches.next()?;
    matches.next().is_none().then(|| (method_key(class, index, method), method))
}

fn same_parameters(left: &JavaMethod, right: &JavaMethod) -> bool {
    left.params().iter().map(|(ty, _)| ty).eq(right.params().iter().map(|(ty, _)| ty))
}

/// Validate every supplied method, including unused helpers.
pub(crate) fn validate_with_receiver(
    program: &JavaProgram, entry: &str, receiver: Option<(&str, &BTreeSet<String>)>, locator: Option<&str>,
) -> Result<SourceEvidence, String> {
    validate_official_namespace(program)?;
    let exceptions = source_exceptions(program)?;
    let layouts = source_layouts(program, receiver)?;
    let ports = source_ports::declarations(program, &exceptions)?;
    if receiver.is_some() {
        external_polling::validate_entry(program, entry, locator.unwrap_or(entry), &ports)?;
    }
    let is_port_interface = |name: &str| ports.values().any(|port| port.interface == name);
    let supported = |ty: &JavaType| {
        scalar(ty) || native_decimal_type(ty) || exact_decimal_type(ty) || calendar_date_type(ty) || matches!(ty, JavaType::Array(item) if integral(item) || declared_reference(item, &layouts) || matches!(item.as_ref(), JavaType::Array(scalar) if integral(scalar))) || declared_reference(ty, &layouts)
            || matches!(ty, JavaType::Class(name) if is_port_interface(name))
            || specialized_interface(program, ty).is_some()
    };
    let mut methods = BTreeMap::new();
    let mut classes = BTreeSet::new();
    let mut nullable_entry_parameters = BTreeSet::new();
    for c in program.classes() {
        let name = c.full_name();
        if !classes.insert(name.clone()) {
            return Err(error("NORM-DUPLICATE-CLASS", &name));
        }
        if c.interfaces().iter().any(|interface| {
            if builtin_resource_interface(program, interface) {
                return closed_resource_close(program, c).is_none();
            }
            let args = c.interface_arguments(interface);
            let ty = if args.is_empty() { JavaType::Class(interface.clone()) }
                else { JavaType::Generic(interface.clone(), args.to_vec()) };
            specialized_interface(program, &ty).is_none_or(|declaration| {
                !plain_source_interface(declaration)
                    || declaration.methods().iter().any(|required| {
                        let count = c.methods().iter().filter(|method| override_matches(required, method)).count();
                        count != 1 && (!required.is_default() || count != 0
                            || c.methods().iter().any(|m| m.name() == required.name() && same_parameters(m, required)))
                    })
            })
        })
            || !unambiguous_interface_defaults(program, c)
            || (!c.is_interface() && !c.type_parameters().is_empty()
                && !(c.fields().is_empty() && c.methods().is_empty())
                && generic_product::prove(c).is_none())
            || (c.is_interface() && !plain_source_interface(c)
                && (!is_port_interface(&name) || !c.fields().is_empty()))
            || !c.annotations().is_empty()
            || !supported_source_imports(c)
            || c.has_unknown_import_origins()
        {
            return Err(error("NORM-CLASS-SHAPE", &name));
        }
        if generic_product::prove(c).is_some() {
            continue;
        }
        for (index, m) in c.methods().iter().enumerate() {
            if c.methods()[..index].iter().any(|prior| prior.name() == m.name()
                && (match (prior.is_constructor(), m.is_constructor()) {
                    (true, true) => prior.arity() == m.arity(),
                    (false, false) => same_parameters(prior, m),
                    _ => true,
                }))
            {
                return Err(error("NORM-OVERLOAD", &format!("{name}.{}", m.name())));
            }
            if c.is_interface() && !m.is_default() { continue; }
            let id = method_key(c, index, m);
            methods.insert(id.clone(), (c, m));
            // A phantom method parameter header does not create a value
            // carrier. Do not let a type parameter shadow a resolvable
            // app or native type and obtain that type's semantics by name.
            if m.type_parameters().iter().any(|name| {
                let ty = JavaType::Class(name.clone());
                supported(&ty) || program.find_class(name).is_some()
            }) || (!m.annotations().is_empty() && !nullable_text_return(c, m))

                || (!m.is_constructor() && !matches!(m.return_type(), JavaType::Void) && !supported(m.return_type()))
                || m.params().iter().any(|(t, _)| !supported(t))
            {
                return Err(error("NORM-METHOD-SHAPE", &id));
            }
            for (ty, parameter) in m.params() {
                let nullable = source_nullable_parameter(c, m, parameter, ty)
                    .map_err(|detail| crate::preprocess::required(
                        "NORM-SOURCE-ANNOTATION", &format!("{id}.{parameter}"), &detail,
                        "请按当前源码合同修改参数注解；不要删除来源语义。",
                    ))?;
                if nullable && id == entry {
                    nullable_entry_parameters.insert(parameter.clone());
                }
            }
            if m.body().is_none() && !m.is_native() {
                return Err(error("NORM-MISSING-BODY", &id));
            }
        }
    }
    if !methods.contains_key(entry) {
        return Err(error("NORM-ENTRY", entry));
    }
    let mut graph = BTreeMap::new();
    let mut exception_bindings = ExceptionBindings::new();
    for (id, &(owner, m)) in &methods {
        let mut calls = Vec::new();
        let Some(body) = m.body() else {
            // A valid native declaration contributes no executable body.
            // An actual call still reaches the kernel's out-of-source Panic.
            graph.insert(id.clone(), Vec::new());
            continue;
        };
        let mut catches = CatchAdmission {
            program,
            parents: &exceptions,
            resolved: exception_bindings.entry(id.clone()).or_default(),
        };
        statements(body, id, &mut calls, &mut catches, false)?;
        let entry_owner = entry.rsplit_once('.').map(|(owner, _)| owner);
        let closed_receiver = receiver.is_some() && entry_owner == Some(owner.full_name().as_str());
        static_types::validate(program, owner, m, id, body, &layouts, closed_receiver)?;
        let mut targets = Vec::new();
        for (call, arity) in calls {
            let (target_owner, target_member) = match call {
                SourceCall::Resolved(key) => {
                    targets.push(key);
                    continue;
                }
                SourceCall::Method { owner, member } => (owner, member),
                SourceCall::StaticReceiver { owner: root, field, member } => {
                    let target = program.find_class(&root)
                        .and_then(|root| crate::kernel_input::inert_instance_method(program, root, &field, &member, arity))
                        .ok_or_else(|| error("NORM-UNBOUND-CALL", &format!("{id} -> {root}.{field}.{member}/{arity}")))?;
                    targets.push(target.2);
                    continue;
                }
                SourceCall::Constructor(owner) => {
                    let class = program
                        .find_class(&owner)
                        .ok_or_else(|| error("NORM-UNBOUND-CALL", &owner))?;
                    if class.is_record() {
                        if arity != class.fields().len() {
                            return Err(error("NORM-RECORD-CONSTRUCTOR", &format!("{id} -> {owner}")));
                        }
                        continue;
                    }
                    if !class.methods().iter().any(|m| m.is_constructor()) && arity == 0 {
                        continue;
                    }
                    let (target, _) = constructor_for_arity(class, arity).ok_or_else(|| error(
                        "NORM-UNBOUND-CALL", &format!("{id} -> {owner} constructor/{arity}")))?;
                    targets.push(target);
                    continue;
                }
            };
            let target = format!("{target_owner}.{target_member}");
            if ports.get(&format!("{}.{target}", owner.full_name())).is_some_and(|port| port.params.len() == arity) {
                continue;
            }
            if !m.params().iter().any(|(_, parameter)| parameter == &target_owner)
                && (crate::normalized_text::source_intrinsic(program, owner, &target, arity).is_some()
                    || crate::normalized_decimal::decimal_intrinsic(program, owner, &target).is_some()) {
                continue;
            }
            // Calls already have their declaring name and member. Preserve
            // the historical first-dot lookup for flattened nested owners.
            let (local, member): (&str, Cow<'_, str>) = match target_owner.split_once('.') {
                Some((local, nested)) => (local, Cow::Owned(format!("{nested}.{target_member}"))),
                None => (target_owner.as_str(), Cow::Borrowed(target_member.as_str())),
            };
            if source_record_local(program, m, local, member.as_ref(), arity) { continue; }
            if let Some(interface) = local_interface_receiver(program, m, local) {
                if interface.methods().iter().filter(|method| method.name() == member.as_ref() && method.arity() == arity).count() != 1 {
                    return Err(error("NORM-UNBOUND-CALL", &format!("{id} -> {target}/{arity}")));
                }
                // Dynamic edges are not the set of all implementations
                // present in this source. Runtime checks the selected
                // allocation, and the active stack still rejects cycles.
                continue;
            }
            if !m.params().iter().any(|(_, parameter)| parameter == &target_owner) {
                if let Some((_, _, key)) = crate::kernel_input::inert_instance_method(program, owner, &target_owner, &target_member, arity) {
                    targets.push(key);
                    continue;
                }
            }
            if program.find_class(&target_owner).is_some_and(|class| {
                class.methods().iter().filter(|method| method.name() == target_member).count() > 1
            }) {
                return Err(crate::preprocess::required(
                    "NORM-OVERLOADED-CALL", id, &format!("no checked call-site dispatch for {target}/{arity}"),
                    "重载入口可用源码参数类型定位；方法体内的重载分派暂不支持，不会猜测目标。",
                ));
            }
            if methods
                .get(&target)
                .is_none_or(|(_, method)| method.arity() != arity)
            {
                return Err(error(
                    "NORM-UNBOUND-CALL",
                    &format!("{id} -> {target}/{arity}"),
                ));
            }
            if target != *id || affine_recursion::prove(m).is_none() {
                targets.push(target);
            }
        }
        graph.insert(id.clone(), targets);
    }
    fn visit(
        id: &str,
        graph: &BTreeMap<String, Vec<String>>,
        active: &mut BTreeSet<String>,
        done: &mut BTreeSet<String>,
    ) -> Result<(), String> {
        if done.contains(id) {
            return Ok(());
        }
        if !active.insert(id.to_string()) {
            return Err(error("NORM-RECURSION", id));
        }
        for next in &graph[id] {
            visit(next, graph, active, done)?;
        }
        active.remove(id);
        done.insert(id.to_string());
        Ok(())
    }
    let mut done = BTreeSet::new();
    for id in graph.keys() {
        visit(id, &graph, &mut BTreeSet::new(), &mut done)?;
    }
    Ok(SourceEvidence { layouts, exception_parents: exceptions, exception_bindings, ports, nullable_entry_parameters })
}

enum SourceCall {
    Resolved(String),
    Method { owner: String, member: String },
    StaticReceiver { owner: String, field: String, member: String },
    Constructor(String),
}

/// Direct Java payload is the explicit String constructor argument, not an
/// observed exception object or a guessed call to getMessage().
pub(crate) enum RuntimeThrowMessage<'a> {
    Absent,
    Null,
    Text(&'a str),
}

pub(crate) fn runtime_throw_message(e: &E) -> Option<RuntimeThrowMessage<'_>> {
    let E::New { java_type: JavaType::Class(class), args } = e else { return None; };
    if !matches!(class.as_str(), "RuntimeException" | RUNTIME_EXCEPTION) { return None; }
    match args.as_slice() {
        [] => Some(RuntimeThrowMessage::Absent),
        [E::Lit(JavaLit::Str(text))] => Some(RuntimeThrowMessage::Text(text)),
        [E::Cast { target_type: JavaType::Class(ty), operand }]
            if ty == "java.lang.String"
                && matches!(operand.as_ref(), E::Lit(JavaLit::Null)) => Some(RuntimeThrowMessage::Null),
        _ => None,
    }
}

/// Per-method exception classes for catches and terminal finally throws.
pub(crate) type ExceptionBindings = BTreeMap<String, BTreeMap<String, String>>;

struct CatchAdmission<'a> {
    program: &'a JavaProgram,
    parents: &'a BTreeMap<String, String>,
    resolved: &'a mut BTreeMap<String, String>,
}
impl CatchAdmission<'_> {
    fn bind(&mut self, at: &str, raw: &str) -> Result<String, String> {
        let owner = at.rsplit_once('.').expect("source method has a declaring class").0;
        let name = exception_name(raw, owner, self.parents).ok_or_else(|| {
            crate::preprocess::required(
                "NORM-CATCH-TYPE", at, &format!("undeclared catch class {raw}"),
                "请在源码入口声明可解析的异常类；不要把非法 catch 当成不匹配。",
            )
        })?;
        self.resolved.insert(raw.into(), name.clone());
        Ok(name)
    }

    fn bind_finally_throw(&mut self, at: &str, raw: &str) -> Result<(), String> {
        let owner = at.rsplit_once('.').map_or("", |(owner, _)| owner);
        let name = exception_name(raw, owner, self.parents)
            .filter(|name| self.parents.contains_key(name))
            .filter(|name| self.program.find_class(name).is_some_and(|class| class.fields().is_empty()))
            .ok_or_else(|| unsupported_finally_throw(at))?;
        self.resolved.insert(raw.into(), name);
        Ok(())
    }
}

fn unsupported_finally_throw(at: &str) -> String {
    crate::preprocess::required(
        "NORM-PROJECT", at, "finally throw requires a source-declared type-only exception",
        "只在 finally 的末尾抛出源码声明且无字段的异常，并使用无参构造。",
    )
}

fn statements(body: &[S], at: &str, calls: &mut Vec<(SourceCall, usize)>, catch_admission: &mut CatchAdmission<'_>, inside_finally: bool) -> Result<(), String> {
    for stmt in body {
        match stmt {
            S::LocalIr(normalizer_ir::local::LocalIr::CountRange { .. }
                | normalizer_ir::local::LocalIr::FoldIndexRange { .. }
                | normalizer_ir::local::LocalIr::MemberChoice { .. }
                | normalizer_ir::local::LocalIr::Stack(_)) => {}
            S::For { .. } if fixed_for::RectangularCounter::from_for(stmt).is_some() => {}
            S::For { .. } if fixed_for::BoundedControlFor::from_for(stmt).is_some() => {}
            S::LoopBreak | S::LoopContinue => return Err(error("NORM-LOOP", at)),
            S::For { cond, body, .. } if fixed_for::FixedFieldFor::from_for(stmt).is_some() => {
                // The proof admits just one literal store and a unit update.
                // The typed source pass checks both field lvalues.
                expr(cond.as_ref().unwrap(), at, calls)?;
                statements(body, at, calls, catch_admission, inside_finally)?;
            }
            S::For {init,cond,update,body}
                if fixed_for::FixedFor::from_header(init.as_deref(),cond.as_ref(),update.as_ref()).is_some() => {
                if let Some(S::Block(declarations)) = init.as_deref() {
                    statements(declarations, at, calls, catch_admission, inside_finally)?;
                }
                statements(body, at, calls, catch_admission, inside_finally)?;
            }
            S::Try { resource, body, catches, finally_block } => {
                if let Some(resource) = resource {
                    let (_, close) = closed_resource_binding(catch_admission.program, resource)
                        .ok_or_else(|| error("NORM-RESOURCE-CLOSURE", at))?;
                    statements(std::slice::from_ref(resource.as_ref()), at, calls, catch_admission, inside_finally)?;
                    calls.push((SourceCall::Resolved(close), 0));
                    if catches.is_empty() && !protected_return(body) && !continuing_try_body(body) {
                        return Err(error("NORM-CATCH-BODY", at));
                    }
                }
                if resource.is_none() || !catches.is_empty() {
                    validate_protected_try(body, catches, finally_block.as_deref(), at)?;
                }
                statements(body, at, calls, catch_admission, inside_finally)?;
                let mut prior_types: Vec<String> = Vec::new();
                for handler in catches {
                    let mut alternatives: Vec<String> = Vec::new();
                    for name in handler.exception_classes() {
                        let class = catch_admission.bind(at, name)?;
                        if let Some(other) = alternatives.iter().find(|other|
                            subtype(&class, other, catch_admission.parents)
                            || subtype(other, &class, catch_admission.parents))
                        {
                            return Err(crate::preprocess::required(
                                "NORM-CATCH-TYPE", at,
                                &format!("multi-catch alternatives {class} and {other} overlap"),
                                "并列的异常类型不能相同，也不能互为父子类。",
                            ));
                        }
                        if let Some(prior) = prior_types.iter().find(|prior|
                            subtype(&class, prior, catch_admission.parents))
                        {
                            return Err(crate::preprocess::required(
                                "NORM-CATCH-ORDER", at, &format!("catch class {class} is already caught by {prior}"),
                                "把更具体的异常处理器放在父类之前，不要删除处理分支。",
                            ));
                        }
                        alternatives.push(class);
                    }
                    prior_types.extend(alternatives);
                    statements(handler.body(), at, calls, catch_admission, inside_finally)?;
                }
                if let Some(finally) = finally_block {
                    let body = match finally.split_last()
                        .and_then(|(last, prefix)| finally_throw(last)
                            .filter(|class| !matches!(*class, "RuntimeException" | crate::normalized::RUNTIME_EXCEPTION))
                            .map(|class| (class, prefix)))
                    {
                        Some((class, prefix)) => {
                            catch_admission.bind_finally_throw(at, class)?;
                            prefix
                        }
                        _ => finally,
                    };
                    statements(body, at, calls, catch_admission, true)?;
                }
            }
            S::LocalIr(normalizer_ir::local::LocalIr::LocalMapEmpty { .. }
                | normalizer_ir::local::LocalIr::LocalMapPut { .. }
                | normalizer_ir::local::LocalIr::LocalMapGet { .. }
                | normalizer_ir::local::LocalIr::ProjectedUnionCount { .. } | normalizer_ir::local::LocalIr::SqlWrite(_) | normalizer_ir::local::LocalIr::Relation(_) | normalizer_ir::local::LocalIr::Decimal(_)) => {}
            S::VarDecl { java_type, initializer: Some(init), .. }
                if host_collection_allocation(java_type, init) => {}
            S::VarDecl { java_type, initializer: Some(init), .. }
                if generic_product::instantiated(catch_admission.program, java_type).is_some() =>
                    expr(init, at, calls)?,
            S::VarDecl {
                java_type,
                initializer: None,
                ..
            } if scalar(java_type) || numeric_array(java_type) || record_array_component(java_type).is_some()
                || matches!(java_type,JavaType::Generic(_,_)|JavaType::Class(_)) => {}
            S::VarDecl {
                java_type,
                initializer: Some(e @ E::Ident(_)),
                ..
            } if record_array_component(java_type).is_some() => expr(e, at, calls)?,
            S::VarDecl {
                java_type,
                initializer: Some(e),
                ..
            } if scalar(java_type) || native_decimal_type(java_type) || matches!(java_type, JavaType::Class(_) | JavaType::Generic(..)) || numeric_array(java_type) => {
                expr(e, at, calls)?
            }
            S::Assign { target: E::ArrayAccess { target, index }, value, .. }
                if constant_array_address(target, index) => {
                expr(target, at, calls)?;
                expr(index, at, calls)?;
                expr(value, at, calls)?;
            }
            S::Assign {
                target: E::FieldAccess { target, .. },
                value,
                ..
            } if matches!(target.as_ref(), E::Ident(_)) => {
                expr(target, at, calls)?;
                expr(value, at, calls)?;
            }
            S::Assign {
                target: E::Ident(_),
                value,
                ..
            } => expr(value, at, calls)?,
            S::Throw(e) if runtime_throw_message(e).is_some() => (),
            S::Throw(e) if !inside_finally => {
                if let Some(class) = finally_throw(stmt) {
                    catch_admission.bind_finally_throw(at, class)?;
                } else {
                    expr(e, at, calls)?;
                    return Err(error("NORM-EXPLICIT-THROW", at));
                }
            }
            S::Throw(_) => return Err(unsupported_finally_throw(at)),
            S::ThisConstructor(args) => {
                let owner = at.rsplit_once('.').expect("constructor key has an owner").0;
                for a in args { expr(a, at, calls)?; }
                calls.push((SourceCall::Constructor(owner.into()), args.len()));
            }
            S::Return(Some(e)) | S::ExprStmt(e) => expr(e, at, calls)?,
            S::Block(b) => statements(b, at, calls, catch_admission, inside_finally)?,
            S::If {
                cond,
                then_block,
                else_block,
            } => {
                expr(cond, at, calls)?;
                statements(then_block, at, calls, catch_admission, inside_finally)?;
                if let Some(b) = else_block {
                    statements(b, at, calls, catch_admission, inside_finally)?;
                }
            }
            S::Switch { selector, cases, default } => {
                expr(selector, at, calls)?;
                for (_, body) in cases {
                    statements(body, at, calls, catch_admission, inside_finally)?;
                }
                statements(default, at, calls, catch_admission, inside_finally)?;
            }
            S::ForEach { var_type, var_name, iterable, body }
                if integral(var_type) && array_sum(var_name, body).is_some() => {
                expr(iterable, at, calls)?;
            }
            S::While { .. } | S::For { .. } | S::ForEach { .. } => {
                return Err(error("NORM-LOOP", at))
            }
            _ => return Err(error("NORM-STATEMENT", at)),
        }
    }
    Ok(())
}
// The parser desugars compound assignments. This bounded source profile
// accepts only addresses with no observable evaluation to duplicate.
fn constant_array_address(target: &E, index: &E) -> bool {
    source_ports::receiver(target).is_some()
        && matches!(index, E::Lit(JavaLit::Int(i)) if *i >= 0)
}

// Read from a bound local or local construction with a signed int literal.
fn source_array_read(target: &E, index: &E) -> bool {
    (matches!(target, E::Ident(_)) || inline_int_array_initializer(target)) && signed_literal_index(index)
}

// Match one full accumulator update and reject every other loop shape at
// one boundary. One closed integer addition to the element is supported.
pub(crate) fn array_sum<'a>(item: &str, body: &'a [S]) -> Option<(&'a str, i64)> {
    let [S::Assign {
        target: E::Ident(total),
        value: E::BinOp { op: B::Add, left, right },
        ..
    }] = body else { return None; };
    if item == total.as_str() { return None; }
    let names = |expression: &E, expected: &str|
        matches!(expression, E::Ident(name) if name == expected);
    let element = if names(left, total) {
        right
    } else if names(right, total) {
        left
    } else {
        return None;
    };
    let offset = match element.as_ref() {
        E::Ident(name) if name == item => Some(0),
        E::BinOp { op: B::Add, left, right } if names(left, item) => integer_token(right),
        E::BinOp { op: B::Add, left, right } if names(right, item) => integer_token(left),
        _ => None,
    }?;
    Some((total.as_str(), offset))
}

fn integer_token(e: &E) -> Option<i64> {
    match e {
        E::Lit(JavaLit::Int(n)) => Some(i64::from(*n)),
        E::Lit(JavaLit::Long(n)) => Some(*n),
        _ => None,
    }
}

pub(crate) fn java_int(ty: &JavaType) -> bool {
    matches!(ty, JavaType::Primitive(name) if name == "int")
}

fn inline_int_array_initializer(e: &E) -> bool {
    matches!(e, E::ArrayInitializer { element_type, .. } if java_int(element_type))
}

fn signed_literal_index(e: &E) -> bool {
    matches!(e, E::Lit(JavaLit::Int(_)))
        || matches!(e, E::UnaryOp { op: U::Neg, operand }
            if matches!(operand.as_ref(), E::Lit(JavaLit::Int(_))))
}

fn expr(e: &E, at: &str, calls: &mut Vec<(SourceCall, usize)>) -> Result<(), String> {
    match e {
        E::Ident(_) | E::Lit(JavaLit::Bool(_) | JavaLit::Str(_) | JavaLit::Null) => (),
        E::Lit(JavaLit::Int(_) | JavaLit::Long(_) | JavaLit::Char(_) | JavaLit::Decimal { .. }) => (),
        E::Cast {
            target_type,
            operand,
        } if matches!(target_type, JavaType::Primitive(n) if n == "long") => {
            expr(operand, at, calls)?
        }
        E::BinOp {
            op: B::Eq | B::Ne | B::Lt | B::Le | B::Gt | B::Ge | B::And | B::Or
                | B::Add | B::Sub | B::Mul | B::Div | B::Mod,
            left, right,
        } => {
            expr(left, at, calls)?;
            expr(right, at, calls)?;
        }
        E::New {
            java_type: JavaType::Class(owner),
            args,
        } => {
            for arg in args {
                expr(arg, at, calls)?;
            }
            calls.push((SourceCall::Constructor(owner.clone()), args.len()));
        }
        E::New { java_type: JavaType::Generic(_, _), args } => {
            // The typed pass binds this source constructor with its actual
            // type arguments; no host or raw generic is dispatched here.
            for arg in args { expr(arg, at, calls)?; }
        }
        E::ArrayInitializer { element_type, elements } if java_int(element_type) => {
            for element in elements {
                expr(element, at, calls)?;
            }
        }
        E::ArrayAllocation { element_type, length } if java_int(element_type) => {
            expr(length, at, calls)?;
        }
        E::FieldAccess { target, .. } => expr(target, at, calls)?,
        E::ArrayAccess { target, index } if source_array_read(target, index) => {
            expr(target, at, calls)?;
            expr(index, at, calls)?;
        }
        E::UnaryOp {
            op: U::Not | U::Neg,
            operand,
        } => expr(operand, at, calls)?,
        E::InstanceOf { value, .. } => expr(value, at, calls)?,
        E::Ternary {
            cond,
            then_expr,
            else_expr,
        } => {
            expr(cond, at, calls)?;
            expr(then_expr, at, calls)?;
            expr(else_expr, at, calls)?;
        }
        E::MethodCall {
            target,
            method,
            args,
        } => {
            let call = match target {
                Some(t) => match t.as_ref() {
                    E::FieldAccess { target, field } => {
                        let E::Ident(owner) = target.as_ref() else { return Err(error("NORM-CALL-TARGET", at)); };
                        if owner == "this" {
                            SourceCall::Method { owner: field.clone(), member: method.clone() }
                        } else {
                            SourceCall::StaticReceiver { owner: owner.clone(), field: field.clone(), member: method.clone() }
                        }
                    }
                    t => SourceCall::Method {
                        owner: source_ports::receiver(t).ok_or_else(|| error("NORM-CALL-TARGET", at))?.into(),
                        member: method.clone(),
                    },
                },
                None => SourceCall::Method { owner: at.rsplit_once('.').unwrap().0.into(), member: method.clone() },
            };
            calls.push((call, args.len()));
            for a in args {
                expr(a, at, calls)?;
            }
        }
        _ => return Err(error("NORM-EXPRESSION", at)),
    }
    Ok(())
}

#[cfg(test)]
mod source_declaration_contracts {
    use super::*;
    use simulator_parser::{parse, ParseOptions};

    #[test]
    fn official_n_annotates_and_admits_text_without_guessing_local_n() {
        let source = r#"
            import normalizer.intrinsics.N;
            class Entry {
                @N.EntryState int prior;
                @N.Nullable static String trim(@N.Nullable String raw) {
                    return N.trim(raw);
                }
            }
        "#;
        let program = parse(source, "Entry.java", &ParseOptions::default()).unwrap();
        let evidence = validate_with_receiver(&program, "Entry.trim", None, None).ok().expect("official binding");
        assert!(evidence.nullable_entry_parameters.contains("raw"));
        let class = program.find_class("Entry").unwrap();
        assert!(entry_state(class, &class.fields()[0]));
        assert!(nullable_text_return(class, &class.methods()[0]));

        let shadowed = format!("{source} class N {{ static String trim(String raw) {{ return raw; }} }}");
        let program = parse(&shadowed, "Entry.java", &ParseOptions::default()).unwrap();
        assert!(validate_with_receiver(&program, "Entry.trim", None, None).err().unwrap().contains("NORM-INTRINSIC-SHADOW"));

        let unbound = parse(
            "class Entry { @N.EntryState int prior; static int read() { return 0; } }",
            "Entry.java", &ParseOptions::default(),
        ).unwrap();
        assert!(validate_with_receiver(&unbound, "Entry.read", None, None).is_err());
    }

    #[test]
    fn a_native_declaration_is_a_port_only_through_an_explicit_receiver() {
        let source = r#"
            import normalizer.intrinsics.N;
            class Entry {
                @N.External private final Client client;
                Entry(Client client) { this.client = client; }
                void notify(int value) { client.notify(value); }
            }
            class Client { native void notify(int value); }
        "#;
        let program = parse(source, "Entry.java", &ParseOptions::default()).unwrap();
        let ports = source_ports::declarations(&program, &BTreeMap::new()).unwrap();
        assert!(ports.contains_key("Entry.client.notify"));
        let unmarked = source.replace("@N.External ", "");
        let program = parse(&unmarked, "Entry.java", &ParseOptions::default()).unwrap();
        assert!(source_ports::declarations(&program, &BTreeMap::new()).unwrap().is_empty());
    }

    #[test]
    fn an_implicit_constructor_only_closes_the_empty_argument_call() {
        let project = |arguments| {
            let source = format!("class Box {{ int value; }} class Entry {{
                static Box build() {{ return new Box({arguments}); }}
            }}");
            parse(&source, "Entry.java", &ParseOptions::default()).unwrap()
        };
        assert!(validate_with_receiver(&project(""), "Entry.build", None, None).is_ok());
        let error = validate_with_receiver(&project("7"), "Entry.build", None, None).err()
            .expect("an implicit constructor never accepts a supplied argument");
        assert_eq!(
            error.lines().next(),
            Some("[NORM-UNBOUND-CALL] kind=PREPROCESS_REQUIRED location=Entry.build -> Box constructor/1"),
            "the attempted constructor and supplied arity must be in the primary diagnostic",
        );
    }

    #[test]
    fn a_native_exact_sum_recognizes_commutation_without_confusing_the_item_and_accumulator() {
        for (write, expected) in [
            ("total = total + row;", Some(("total", 0))),
            ("total = row + total;", Some(("total", 0))),
            ("total = (1 + row) + total;", Some(("total", 1))),
            ("row = row + row;", None),
            ("total = other + row;", None),
            ("total = total + other;", None),
            ("total = total + (other + 1);", None),
        ] {
            let source = format!("class Sum {{ static int run(int[] rows, int other) {{
                int total = 0; for (int row : rows) {{ {write} }} return total;
            }} }}");
            let program = parse(&source, "Sum.java", &ParseOptions::default()).unwrap();
            let method = &program.find_class("Sum").unwrap().methods()[0];
            let [_, S::ForEach { var_name, body, .. }, _] = method.body().unwrap() else {
                panic!("the source must keep one enhanced for loop");
            };
            assert_eq!(array_sum(var_name, body), expected, "{write}");
        }
    }

    #[test]
    fn numeric_array_admission_does_not_prove_boolean_element_arithmetic() {
        let array = |component: &str| JavaType::Array(Box::new(JavaType::Primitive(component.into())));
        assert!(numeric_array(&array("int")));
        assert!(numeric_array(&array("long")));
        assert!(!numeric_array(&array("boolean")));
    }

    #[test]
    fn qualified_and_short_object_fields_remain_opaque_reference_layouts() {
        let program = parse(
            "class Holder { Object shortName; java.lang.Object fullName; Child child; } class Child { int value; }",
            "Holder.java",
            &ParseOptions::default(),
        ).unwrap();
        let layouts = source_layouts(&program, None).unwrap();
        let holder = &layouts["Holder"];
        for name in ["shortName", "fullName", "child"] {
            assert_eq!(holder[name], "ref?");
        }
    }

    #[test]
    fn ordinary_interface_eligibility_does_not_assume_every_interface_is_a_contract() {
        for (source, expected) in [
            ("interface Shape { int size(); }", true),
            ("interface Shape {}", false),
            ("interface Shape { int VERSION = 1; int size(); }", false),
            ("interface Shape { static int size() { return 1; } }", false),
            ("interface Shape { default int size() { return 1; } }", true),
            ("interface Shape { @Deprecated int size(); }", false),
        ] {
            let program = parse(source, "Shape.java", &ParseOptions::default()).unwrap();
            assert_eq!(plain_source_interface(program.find_class("Shape").unwrap()), expected, "{source}");
        }
        let constructor = parse("interface Shape { Shape() {} }", "Shape.java", &ParseOptions::default())
            .expect_err("a constructor in an interface is rejected at parse admission");
        assert!(constructor.to_string().contains("[JAVA-CONSTRUCTOR] interface cannot declare a constructor"));
    }

    #[test]
    fn declared_external_payloads_do_not_infer_a_value_from_unrelated_fields() {
        for (imports, field, expected) in [
            ("", "int payload;", Some("number")),
            ("import normalizer.intrinsics.N;", "@N.Nullable java.lang.String payload;", Some("text?")),
            ("", "@normalizer.intrinsics.N.Nullable String payload;", Some("text?")),
            ("", "String payload;", None),
            ("", "@N.Nullable String payload;", None),
            ("import normalizer.intrinsics.N;", "@N.Nullable int payload;", None),
            ("import normalizer.intrinsics.N;", "@N.Nullable(1) String payload;", None),
            ("", "int other;", None),
            ("", "static int payload;", None),
            ("", "final int payload = 1;", None),
            ("", "int payload = 1;", None),
        ] {
            let source = format!("{imports} class RemoteFailure extends RuntimeException {{ {field} }}");
            let program = parse(&source, "RemoteFailure.java", &ParseOptions::default()).unwrap();
            assert_eq!(source_payload_domain(program.find_class("RemoteFailure").unwrap()), expected);
            if expected.is_none() {
                assert!(source_exceptions(&program).unwrap_err().contains("[NORM-EXCEPTION-DECLARATION]"));
            }
        }
    }
}
