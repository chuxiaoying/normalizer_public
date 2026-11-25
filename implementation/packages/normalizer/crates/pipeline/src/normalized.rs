//! First, deliberately small normalized input profile. No plugin discovery.
//! This structural gate is not a Java type checker or a translation proof.
mod external_polling;
pub(crate) mod fixed_for;
pub(crate) mod source_ports;

use simulator_parser::{
    JavaBinOp as B, JavaExpr as E, JavaLit, JavaProgram, JavaStatement as S, JavaType,
    JavaUnaryOp as U,
};
use std::collections::{BTreeMap, BTreeSet};

fn error(code: &str, at: &str) -> String {
    let action = match code {
        "NORM-LOOP" => "按 Skill 将循环改写为固定遍历语义并保留状态、调用顺序及异常；Rust 尚未接入 fold_until，请报告能力缺口，不要伪造无循环实现。",
        "NORM-RECURSION" => "按 Skill 消解递归调用环；无法用已有固定语义表达时报告能力缺口，不要截断递归深度。",
        "NORM-UNBOUND-CALL" | "NORM-MISSING-BODY" => "按 Skill 提供闭合函数体或真实外部交互依据；不得用空实现、默认值或伪造输入隐藏缺口。",
        _ => "按 Skill 对照当前 scalar-0 能力准备规范 Java；超出当前能力的类型、状态或语义绑定应报告缺口，不得删除业务行为。",
    };
    crate::preprocess::required(code, at, "输入不满足当前规范 Java 合同。", action)
}
pub(crate) fn integral(t: &JavaType) -> bool {
    matches!(t, JavaType::Primitive(n) if matches!(n.as_str(), "byte" | "short" | "int" | "long"))
}
pub(crate) fn exact_decimal_type(ty: &JavaType) -> bool {
    // Fully qualified reserved carrier, with exact decimal value semantics
    // only for this profile. java.lang.Double / native double stay separate.
    matches!(ty, JavaType::Class(name) if name == "java.math.BigDecimal")
}

pub(crate) fn numeric_array(ty: &JavaType) -> bool {
    matches!(ty, JavaType::Array(item) if integral(item))
}

fn scalar(t: &JavaType) -> bool {
    integral(t)
        || matches!(t, JavaType::Primitive(n) if n == "boolean")
        || matches!(t, JavaType::Class(n) if matches!(n.as_str(), "Object" | "java.lang.Object" | "String" | "java.lang.String"))
}

pub(crate) fn plain_source_interface(class: &simulator_parser::JavaClass) -> bool {
    class.is_interface()
        && class.fields().is_empty()
        && class.supertype().is_none()
        && class.interfaces().is_empty()
        && !class.methods().is_empty()
        && class.methods().iter().all(|method| !method.is_static() && !method.is_constructor()
            && !method.is_varargs() && method.body().is_none() && method.throws().is_empty()
            && method.annotations().is_empty() && method.type_params().is_empty()
            && scalar(method.return_type()) && method.params().iter().all(|(ty, _)| scalar(ty)))
}

fn override_matches(interface: &simulator_parser::JavaMethod, concrete: &simulator_parser::JavaMethod) -> bool {
    interface.name() == concrete.name() && !concrete.is_static() && concrete.body().is_some()
        && interface.return_type() == concrete.return_type()
        && interface.params().iter().map(|(ty, _)| ty).eq(concrete.params().iter().map(|(ty, _)| ty))
}

fn local_interface_receiver<'a>(
    program: &'a JavaProgram, method: &simulator_parser::JavaMethod, local: &str,
) -> Option<&'a simulator_parser::JavaClass> {
    let declared = method.params().iter().find(|(_, name)| name == local).map(|(ty, _)| ty)
        .or_else(|| method.body()?.iter().find_map(|statement| match statement {
            S::VarDecl { java_type, name, .. } if name == local => Some(java_type),
            _ => None,
        }))?;
    let JavaType::Class(name) = declared else { return None };
    program.find_class(name).filter(|class| plain_source_interface(class))
}

/// Source layouts retain declaring owners. Primitive DTO fields follow the
/// business numeric domain and the existing Object carrier remains opaque.
pub(crate) fn source_layouts(
    program: &JavaProgram,
) -> Result<BTreeMap<String, BTreeMap<String, String>>, String> {
    let mut layouts = BTreeMap::new();
    for class in program.classes() {
        if source_payload_domain(class).is_some() { continue; }
        if class.fields().is_empty() {
            if plain_source_interface(class) || !class.interfaces().is_empty() {
                layouts.insert(class.full_name(), BTreeMap::new());
            }
            continue;
        }
        let mut fields = BTreeMap::new();
        for field in class.fields() {
            if source_ports::marked(class, field) {
                continue;
            }
            let annotation_ok = field.annotations().is_empty() || entry_state(class, field) || source_nullable_field(class, field);
            if field.is_static()
                || field.is_final()
                || field.is_volatile()
                || field.initializer().is_some()
                || !annotation_ok
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
            if fields.insert(field.name().into(), ty.into()).is_some() {
                return Err(error("NORM-FIELD-DECLARATION", &class.full_name()));
            }
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
pub(crate) fn entry_state(
    class: &simulator_parser::JavaClass,
    field: &simulator_parser::JavaField,
) -> bool {
    let [annotation] = field.annotations() else {
        return false;
    };
    !annotation.has_arguments()
        && (annotation.name() == "normalizer.annotations.EntryState"
            || (annotation.name() == "EntryState"
                && class
                    .imports()
                    .iter()
                    .any(|name| name == "normalizer.annotations.EntryState")))
}
pub(crate) fn source_nullable_field(
    class: &simulator_parser::JavaClass, field: &simulator_parser::JavaField,
) -> bool {
    let [annotation] = field.annotations() else { return false; };
    matches!(field.java_type(), JavaType::Class(_))
        && !annotation.has_arguments()
        && (annotation.name() == "normalizer.annotations.Nullable"
            || (annotation.name() == "Nullable" && class.imports().iter().any(|name| name == "normalizer.annotations.Nullable")))
}

pub(crate) fn nullable_text_field(
    class: &simulator_parser::JavaClass, field: &simulator_parser::JavaField,
) -> bool {
    matches!(field.java_type(), JavaType::Class(name) if name == "String" || name == "java.lang.String")
        && source_nullable_field(class, field)
}
pub(crate) fn nullable_text_return(class: &simulator_parser::JavaClass, method: &simulator_parser::JavaMethod) -> bool {
    let [annotation] = method.annotations() else { return false; };
    matches!(method.return_type(), JavaType::Class(name) if name == "String" || name == "java.lang.String")
        && !annotation.has_arguments()
        && (annotation.name() == "normalizer.annotations.Nullable"
            || (annotation.name() == "Nullable" && class.imports().iter().any(|name| name == "normalizer.annotations.Nullable")))
}

pub(crate) fn supported_source_imports(class: &simulator_parser::JavaClass) -> bool {
    class.imports().iter().all(|name| {
        matches!(
            name.as_str(),
            source_ports::IMPORT | "normalizer.annotations.EntryState" | "normalizer.annotations.Nullable" | "java.util.List" | crate::normalized_text::SOURCE_IMPORT
        )
    })
}
fn declared_reference(t: &JavaType, layouts: &BTreeMap<String, BTreeMap<String, String>>) -> bool {
    matches!(t, JavaType::Class(name) if layouts.keys().any(|owner| owner == name || owner.rsplit('.').next() == Some(name.as_str())))
}

pub(crate) const RUNTIME_EXCEPTION: &str = "java.lang.RuntimeException";
pub(crate) fn exception_name(name: &str, owner: &str, parents: &BTreeMap<String, String>) -> Option<String> {
    if matches!(name, "RuntimeException" | "java.lang.RuntimeException") {
        return Some(RUNTIME_EXCEPTION.into());
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
    if field.name() != "payload" || field.is_static() || field.is_final() || field.is_volatile()
        || field.initializer().is_some() || !field.annotations().is_empty()
    {
        return None;
    }
    match field.java_type() {
        JavaType::Class(name) if matches!(name.as_str(), "String" | "java.lang.String") => Some("text?"),
        ty if integral(ty) => Some("number"),
        _ => None,
    }
}

pub(crate) fn source_exceptions(program: &JavaProgram) -> Result<BTreeMap<String, String>, String> {
    let mut parents = BTreeMap::new();
    for class in program.classes() {
        // The tiny builtin hierarchy is authoritative only when source cannot
        // shadow one of its names. Reject rather than guess lexical rebinding.
        if matches!(
            class.simple_name().rsplit('.').next().unwrap(),
            "RuntimeException"
                | "NullPointerException"
                | "ArithmeticException"
                | "IndexOutOfBoundsException"
                | "ArrayIndexOutOfBoundsException"
                | "NegativeArraySizeException"
        ) {
            return Err(error("NORM-EXCEPTION-SHADOW", &class.full_name()));
        }
        if let Some(parent) = class.supertype() {
            if (!class.fields().is_empty() && source_payload_domain(class).is_none())
                || !class.methods().is_empty()
                || !class.annotations().is_empty()
                || !class.interfaces().is_empty()
                || !class.type_params().is_empty()
                || class.is_interface()
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
        while current != RUNTIME_EXCEPTION {
            if !seen.insert(current) {
                return Err(error("NORM-EXCEPTION-CYCLE", name));
            }
            current = parents
                .get(current)
                .ok_or_else(|| error("NORM-EXCEPTION-BASE", name))?;
        }
    }
    Ok(parents)
}
pub(crate) fn subtype(child: &str, parent: &str, parents: &BTreeMap<String, String>) -> bool {
    if child == parent {
        return true;
    }
    if child == "java.lang.ArrayIndexOutOfBoundsException" {
        return matches!(parent, "java.lang.IndexOutOfBoundsException" | RUNTIME_EXCEPTION);
    }
    if matches!(
        child,
        "java.lang.NullPointerException"
            | "java.lang.ArithmeticException"
            | "java.lang.IndexOutOfBoundsException"
            | "java.lang.NegativeArraySizeException"
    ) {
        return parent == RUNTIME_EXCEPTION;
    }
    let mut current = child;
    while let Some(next) = parents.get(current) {
        if next == parent {
            return true;
        }
        current = next;
    }
    false
}
/// A protected expression may bind local temporaries in evaluation order. An
/// exception can leave at any binding. Such temporaries are out of scope in
/// every handler, so their partial values need no exceptional snapshot.
pub(crate) fn protected_return(body: &[S]) -> bool {
    matches!(body.split_last(), Some((S::Return(Some(_)), prefix))
        if prefix.iter().all(|s| matches!(s, S::VarDecl { initializer: Some(_), .. })))
}

fn validate_protected_try(body: &[S], id: &str) -> Result<(), String> {
    let [S::Try { body, catches, finally_block }] = body else { return Ok(()); };
    if protected_return(body) && (finally_block.is_none()
        || (!catches.is_empty() && catches.iter().all(|handler| protected_return(handler.body()))))
    {
        return Ok(());
    }
    Err(error("NORM-CATCH-BODY", id))
}

/// Validate ALL supplied methods, including unused helpers. Explicitly reject
/// unsupported shapes instead of sending them to legacy semantic inference.
pub fn validate(program: &JavaProgram, entry: &str) -> Result<(), String> {
    external_polling::validate_entry(program, entry)?;
    let exceptions = source_exceptions(program)?;
    let layouts = source_layouts(program)?;
    let ports = source_ports::declarations(program, &exceptions)?;
    let is_port_interface = |name: &str| ports.values().any(|port| port.interface == name);
    let supported = |ty: &JavaType| {
        scalar(ty) || exact_decimal_type(ty) || matches!(ty, JavaType::Array(item) if integral(item) || declared_reference(item, &layouts) || matches!(item.as_ref(), JavaType::Array(scalar) if integral(scalar))) || declared_reference(ty, &layouts)
            || matches!(ty, JavaType::Class(name) if is_port_interface(name))
    };
    let mut methods = BTreeMap::new();
    let mut classes = BTreeSet::new();
    for c in program.classes() {
        let name = c.full_name();
        if !classes.insert(name.clone()) {
            return Err(error("NORM-DUPLICATE-CLASS", &name));
        }
        if !c.package().is_empty()
            || (c.supertype().is_some() && !exceptions.contains_key(&name))
            || c.interfaces().iter().any(|interface| {
                program.find_class(interface).is_none_or(|declaration| {
                    !plain_source_interface(declaration)
                        || declaration.methods().iter().any(|required| {
                            c.methods().iter().filter(|method| override_matches(required, method)).count() != 1
                        })
                })
            })
            || (c.is_interface() && !plain_source_interface(c)
                && (!is_port_interface(&name) || !c.fields().is_empty()))
            || (!c.fields().is_empty() && !layouts.contains_key(&name) && !exceptions.contains_key(&name))
            || !c.annotations().is_empty()
            || !c.type_params().is_empty()
            || !supported_source_imports(c)
            || !c.imports_wildcard().is_empty()
            || !c.static_imports().is_empty()
            || !c.static_imports_wildcard().is_empty()
        {
            return Err(error("NORM-CLASS-SHAPE", &name));
        }
        if c.is_interface() {
            continue;
        }
        for m in c.methods() {
            let id = format!("{}.{}", name, m.name());
            if methods.insert(id.clone(), m).is_some() {
                return Err(error("NORM-OVERLOAD", &id));
            }
            if m.is_varargs()
                || (!m.annotations().is_empty() && !nullable_text_return(c, m))
                || !m.type_params().is_empty()
                || (!m.is_constructor() && !supported(m.return_type()))
                || m.params().iter().any(|(t, _)| !supported(t))
            {
                return Err(error("NORM-METHOD-SHAPE", &id));
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
    for (id, m) in &methods {
        let mut calls = Vec::new();
        let Some(body) = m.body() else {
            // A valid native declaration contributes no executable body.
            // An actual call still reaches the kernel's out-of-source Panic.
            graph.insert(id.clone(), Vec::new());
            continue;
        };
        validate_protected_try(body, id)?;
        statements(body, id, &mut calls)?;
        let mut targets = Vec::new();
        for (call, arity) in calls {
            let target = match call {
                SourceCall::Method(id) => id,
                SourceCall::Constructor(owner) => {
                    let class = program
                        .find_class(&owner)
                        .ok_or_else(|| error("NORM-UNBOUND-CALL", &owner))?;
                    if !class.methods().iter().any(|m| m.is_constructor()) && arity == 0 {
                        continue;
                    }
                    format!("{owner}.{owner}")
                }
            };
            let caller = program.find_class(id.rsplit_once('.').unwrap().0).unwrap();
            if ports.get(&format!("{}.{target}", caller.full_name())).is_some_and(|port| port.params.len() == arity) {
                continue;
            }
            if crate::normalized_text::source_intrinsic(caller, &target, arity) {
                continue;
            }
            if let Some((local, member)) = target.split_once('.') {
                if let Some(interface) = local_interface_receiver(program, m, local) {
                    if interface.methods().iter().filter(|method| method.name() == member && method.arity() == arity).count() != 1 {
                        return Err(error("NORM-UNBOUND-CALL", &format!("{id} -> {target}/{arity}")));
                    }
                    // Dynamic edges are not the set of all implementations
                    // present in this source. Runtime checks the selected
                    // allocation, and the active stack still rejects cycles.
                    continue;
                }
            }
            if methods
                .get(&target)
                .is_none_or(|method| method.arity() != arity)
            {
                return Err(error(
                    "NORM-UNBOUND-CALL",
                    &format!("{id} -> {target}/{arity}"),
                ));
            }
            targets.push(target);
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
    Ok(())
}

enum SourceCall {
    Method(String),
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

fn statements(body: &[S], at: &str, calls: &mut Vec<(SourceCall, usize)>) -> Result<(), String> {
    for stmt in body {
        match stmt {
            S::LocalIr(normalizer_ir::local::LocalIr::CountRange { .. }
                | normalizer_ir::local::LocalIr::FoldIndexRange { .. }
                | normalizer_ir::local::LocalIr::MemberChoice { .. }
                | normalizer_ir::local::LocalIr::Stack(_)) => {}
            S::For { .. } if fixed_for::RectangularCounter::from_for(stmt).is_some() => {}
            S::For {init,cond,update,body}
                if fixed_for::FixedFor::from_header(init.as_deref(),cond.as_ref(),update.as_ref()).is_some() => {
                statements(body, at, calls)?;
            }
            S::Try { body, catches, finally_block } if !catches.is_empty() => {
                statements(body, at, calls)?;
                for handler in catches { statements(handler.body(), at, calls)?; }
                if let Some(finally) = finally_block { statements(finally, at, calls)?; }
            }
            S::LocalIr(normalizer_ir::local::LocalIr::LocalMapEmpty { .. }
                | normalizer_ir::local::LocalIr::LocalMapPut { .. }
                | normalizer_ir::local::LocalIr::LocalMapGet { .. }
                | normalizer_ir::local::LocalIr::ProjectedUnionCount { .. } | normalizer_ir::local::LocalIr::SqlWrite(_) | normalizer_ir::local::LocalIr::Relation(_) | normalizer_ir::local::LocalIr::Decimal(_)) => {}
            S::LocalIr(normalizer_ir::local::LocalIr::Unsupported(_)) => {
                return Err(error("NORM-UNSUPPORTED-LOCAL-IR", at));
            }
            S::VarDecl {
                java_type,
                initializer: None,
                ..
            } if scalar(java_type) || numeric_array(java_type) || matches!(java_type,JavaType::Generic(_,_)|JavaType::Class(_)) => {}
            S::VarDecl {
                java_type,
                initializer: Some(e),
                ..
            } if scalar(java_type) || matches!(java_type, JavaType::Class(_)) || numeric_array(java_type) => {
                expr(e, at, calls)?
            }
            S::Assign { target: E::ArrayAccess { target, index }, value }
                if constant_array_address(target, index) => {
                expr(target, at, calls)?;
                expr(index, at, calls)?;
                expr(value, at, calls)?;
            }
            S::Assign {
                target: E::FieldAccess { target, .. },
                value,
            } if matches!(target.as_ref(), E::Ident(_)) => {
                expr(target, at, calls)?;
                expr(value, at, calls)?;
            }
            S::Assign {
                target: E::Ident(_),
                value,
            } => expr(value, at, calls)?,
            S::Throw(e) if runtime_throw_message(e).is_some() => (),
            S::Return(Some(e)) | S::ExprStmt(e) => expr(e, at, calls)?,
            S::Block(b) | S::SameScope(b) => statements(b, at, calls)?,
            S::If {
                cond,
                then_block,
                else_block,
            } => {
                expr(cond, at, calls)?;
                statements(then_block, at, calls)?;
                if let Some(b) = else_block {
                    statements(b, at, calls)?;
                }
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
    let local = matches!(target, E::Ident(_));
    let entry_field = matches!(target, E::FieldAccess { target, .. }
        if matches!(target.as_ref(), E::Ident(owner) if owner == "this"));
    (local || entry_field) && matches!(index, E::Lit(JavaLit::Int(i)) if *i >= 0)
}

// Only a single exact sum update with an element coefficient of one.
pub(crate) fn array_sum<'a>(item: &str, body: &'a [S]) -> Option<(&'a str, i64)> {
    let [S::Assign { target: E::Ident(total), value: E::BinOp { op: B::Add, left, right } }] = body else {
        return None;
    };
    if item == total || !matches!(left.as_ref(), E::Ident(name) if name == total) {
        return None;
    }
    let delta = match right.as_ref() {
        E::Ident(n) if n == item => 0,
        E::BinOp { op: op @ (B::Add | B::Sub), left, right }
            if matches!(left.as_ref(), E::Ident(n) if n == item) => {
            let amount = match right.as_ref() {
                E::Lit(JavaLit::Int(n)) => i64::from(*n),
                E::Lit(JavaLit::Long(n)) => *n,
                _ => return None,
            };
            if *op == B::Sub { amount.checked_neg()? } else { amount }
        }
        _ => return None,
    };
    Some((total, delta))
}

pub(crate) fn java_int(ty: &JavaType) -> bool {
    matches!(ty, JavaType::Primitive(name) if name == "int")
}

fn local_array_construction(e: &E) -> bool {
    matches!(e, E::ArrayInitializer { element_type, .. } | E::ArrayAllocation { element_type, .. }
        if java_int(element_type))
}

fn signed_literal_index(e: &E) -> bool {
    match e {
        E::Lit(JavaLit::Int(_)) => true,
        E::UnaryOp { op: U::Neg, operand } => matches!(operand.as_ref(), E::Lit(JavaLit::Int(_))),
        _ => false,
    }
}

fn expr(e: &E, at: &str, calls: &mut Vec<(SourceCall, usize)>) -> Result<(), String> {
    match e {
        E::Ident(_) | E::Lit(JavaLit::Bool(_) | JavaLit::Str(_) | JavaLit::Null) => (),
        E::Lit(JavaLit::Int(_) | JavaLit::Long(_) | JavaLit::Char(_)) => (),
        E::Cast {
            target_type,
            operand,
        } if matches!(target_type, JavaType::Primitive(n) if n == "long") => {
            expr(operand, at, calls)?
        }
        // A source declaration may contain a known nonzero divisor without
        // assigning the division a PERS result. If this body is reached, the
        // executor still requires a supported division contract.
        E::BinOp { op: B::Div, left, right } if matches!(right.as_ref(), E::Lit(JavaLit::Int(n)) if *n != 0) => {
            expr(left, at, calls)?;
        }
        E::BinOp { op, left, right }
            if matches!(
                op,
                B::Eq
                    | B::Ne
                    | B::Lt
                    | B::Le
                    | B::Gt
                    | B::Ge
                    | B::And
                    | B::Or
                    | B::Add
                    | B::Sub
                    | B::Mul
            ) =>
        {
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
        E::ArrayInitializer { element_type, elements } if java_int(element_type) => {
            for element in elements {
                expr(element, at, calls)?;
            }
        }
        E::ArrayAllocation { element_type, length } if java_int(element_type) => {
            expr(length, at, calls)?;
        }
        E::FieldAccess { target, .. } => expr(target, at, calls)?,
        E::ArrayAccess { target, index } if constant_array_address(target, index)
            || (local_array_construction(target) || matches!(target.as_ref(), E::Ident(_)))
                && signed_literal_index(index) => {
            expr(target, at, calls)?;
            expr(index, at, calls)?;
        }
        E::UnaryOp {
            op: U::Not | U::Neg,
            operand,
        } => expr(operand, at, calls)?,
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
            let owner = match target {
                None => at.rsplit_once('.').unwrap().0,
                Some(t) => source_ports::receiver(t).ok_or_else(|| error("NORM-CALL-TARGET", at))?,
            };
            calls.push((SourceCall::Method(format!("{owner}.{method}")), args.len()));
            for a in args {
                expr(a, at, calls)?;
            }
        }
        _ => return Err(error("NORM-EXPRESSION", at)),
    }
    Ok(())
}
