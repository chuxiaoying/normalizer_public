//! First, deliberately small normalized input profile. No plugin discovery.
//! This structural gate is not a Java type checker or a translation proof.
pub(crate) mod affine_recursion;
mod external_polling;
pub(crate) mod fixed_for;
pub(crate) mod generic_product;
pub(crate) mod source_ports;
mod static_types;

use simulator_parser::{
    AnnotationRef, JavaBinOp as B, JavaClass, JavaExpr as E, JavaLit, JavaMethod, JavaProgram,
    JavaStatement as S, JavaType, JavaUnaryOp as U,
};
use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
};

fn error(code: &str, at: &str) -> String {
    let action = match code {
        "NORM-LOOP" => "先核实源码是否有可证明的循环边界。合规原生循环不应为绕过内核缺口而改写；其余仅在能证明等价时交付正式扩展语义，并保留状态、调用顺序和异常。",
        "NORM-RECURSION" => "按 Skill 消解递归调用环；无法用已有固定语义表达时报告能力缺口，不要截断递归深度。",
        "NORM-UNBOUND-CALL" | "NORM-MISSING-BODY" => "按 Skill 提供闭合函数体或真实外部交互依据；不得用空实现、默认值或伪造输入隐藏缺口。",
        _ => "按纯净工程规范交付源码；遇到内核尚不能分析的类型、状态或语义时报告缺口，不得删除业务行为。",
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
            E::Lit(JavaLit::Char(unit)) => Some((i128::from(*unit), false)),
            E::UnaryOp {
                op: U::Neg,
                operand,
            } => integer(operand).map(|(value, long)| (-value, long)),
            _ => None,
        }
    }
    match (t, e) {
        (JavaType::Primitive(ty), e) if integral(t) => {
            let (value, long) = integer(e)?;
            let fits = match ty.as_str() {
                "byte" => !long && i8::try_from(value).is_ok(),
                "short" => !long && i16::try_from(value).is_ok(),
                "char" => !long && u16::try_from(value).is_ok(),
                "int" => !long && i32::try_from(value).is_ok(),
                "long" => i64::try_from(value).is_ok(),
                _ => false,
            };
            fits.then(|| b::lit_integer(value))
        }
        (JavaType::Primitive(ty), E::Lit(JavaLit::Bool(value))) if ty == "boolean" => {
            Some(b::lit_bool(*value))
        }
        (JavaType::Class(ty), E::Lit(JavaLit::Str(value)))
            if matches!(ty.as_str(), "String" | "java.lang.String") =>
        {
            Some(b::lit_str(value))
        }
        (JavaType::Class(ty), E::Lit(JavaLit::StrUnits(units)))
            if matches!(ty.as_str(), "String" | "java.lang.String") =>
        {
            Some(b::lit(math::Lit::from_text_units(units.clone())))
        }
        (JavaType::Class(_), E::Lit(JavaLit::Null)) => Some(b::lit_null()),
        _ => None,
    }
}

/// A well-typed host-only declaration may exist in an uncalled method. The
/// type relation here is declaration evidence, not a library implementation;
/// actual execution of the allocation must still fail closed.
pub(crate) fn host_collection_allocation(declared: &JavaType, init: &E) -> bool {
    let JavaType::Generic(interface, element) = declared else {
        return false;
    };
    if interface != "java.util.List"
        || element.len() != 1
        || !matches!(&element[0], JavaType::Class(name) if matches!(name.as_str(), "java.lang.String" | "String"))
    {
        return false;
    }
    matches!(init, E::New { java_type: JavaType::Generic(allocator, diamond), args }
        if allocator == "java.util.ArrayList" && diamond.is_empty() && args.is_empty())
}

pub(crate) fn integral(t: &JavaType) -> bool {
    matches!(t, JavaType::Primitive(n) if matches!(n.as_str(), "byte" | "short" | "char" | "int" | "long"))
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

// The wide carrier is the Java platform Object, not an application
// declaration that happens to use the same short name.
pub(crate) fn root_object_type(program: &JavaProgram, ty: &JavaType) -> bool {
    let JavaType::Class(name) = ty else {
        return false;
    };
    name == "java.lang.Object"
        || (name == "Object"
            && !program
                .classes()
                .iter()
                .any(|class| class.full_name().rsplit('.').next() == Some("Object")))
}

pub(crate) fn platform_string_type(program: &JavaProgram, ty: &JavaType) -> bool {
    let JavaType::Class(name) = ty else {
        return false;
    };
    name == "java.lang.String"
        || (name == "String"
            && !program
                .classes()
                .iter()
                .any(|class| class.full_name().rsplit('.').next() == Some("String")))
}

pub(crate) fn numeric_array(ty: &JavaType) -> bool {
    matches!(ty, JavaType::Array(item) if integral(item))
}

fn record_array_component(ty: &JavaType) -> Option<&JavaType> {
    match ty {
        JavaType::Array(component) if matches!(component.as_ref(), JavaType::Class(_)) => {
            Some(component)
        }
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
        && (!class.methods().is_empty() || !class.interfaces().is_empty())
        && class.methods().iter().all(|method| {
            !method.is_static()
                && !method.is_private()
                && method.body().is_some() == method.is_default()
                && method.throws().is_empty()
                && method.annotations().is_empty()
                && method.type_parameters().is_empty()
                && scalar(method.return_type())
                && method.params().iter().all(|(ty, _)| scalar(ty))
        })
}

pub(crate) fn override_matches(
    interface: &simulator_parser::JavaMethod,
    concrete: &simulator_parser::JavaMethod,
) -> bool {
    interface.name() == concrete.name()
        && !concrete.is_static()
        && concrete.is_public()
        && concrete.body().is_some()
        && interface.return_type() == concrete.return_type()
        && interface
            .params()
            .iter()
            .map(|(ty, _)| ty)
            .eq(concrete.params().iter().map(|(ty, _)| ty))
}

pub(crate) fn source_record_accessor<'a>(
    class: &'a JavaClass,
    member: &str,
    arity: usize,
) -> Option<(&'a str, &'a JavaType)> {
    if arity != 0 {
        return None;
    }
    let field_name = if let Some((_, method)) = named_method_for_arity(class, member, arity) {
        if method.is_static() {
            return None;
        }
        let [S::Return(Some(returned))] = method.body()? else {
            return None;
        };
        let name = match returned {
            E::Ident(name) => name,
            E::FieldAccess { target, field } if matches!(target.as_ref(), E::Ident(name) if name == "this") => {
                field
            }
            _ => return None,
        };
        let field = class
            .fields()
            .iter()
            .find(|field| !field.is_static() && field.name() == name)?;
        return (method.return_type() == field.java_type())
            .then(|| (field.name(), field.java_type()));
    } else if class.is_record() {
        member
    } else {
        return None;
    };
    class
        .fields()
        .iter()
        .find(|field| !field.is_static() && field.name() == field_name)
        .map(|field| (field.name(), field.java_type()))
}

pub(crate) fn plain_source_data_method(class: &JavaClass, method: &JavaMethod) -> bool {
    if method.is_static() || method.body().is_none() {
        return false;
    }
    if method.is_constructor() {
        return true;
    }
    if source_record_accessor(class, method.name(), method.arity()).is_some() {
        return true;
    }
    let [(input_type, input)] = method.params() else {
        return false;
    };
    let Some(
        [S::Assign {
            target,
            value: E::Ident(value),
            is_compound: false,
        }],
    ) = method.body()
    else {
        return false;
    };
    let field = match target {
        E::Ident(field) if field != input => field,
        E::FieldAccess { target, field } if matches!(target.as_ref(), E::Ident(name) if name == "this") => {
            field
        }
        _ => return false,
    };
    value == input
        && matches!(method.return_type(), JavaType::Void)
        && class.fields().iter().any(|declared| {
            !declared.is_static() && declared.name() == field && declared.java_type() == input_type
        })
}

fn source_local_class<'a>(
    program: &'a JavaProgram,
    method: &JavaMethod,
    local: &str,
) -> Option<&'a JavaClass> {
    let declared = method
        .params()
        .iter()
        .find(|(_, name)| name == local)
        .map(|(ty, _)| ty)
        .or_else(|| {
            method.body()?.iter().find_map(|statement| match statement {
                S::VarDecl {
                    java_type, name, ..
                } if name == local => Some(java_type),
                _ => None,
            })
        });
    let Some(JavaType::Class(owner)) = declared else {
        return None;
    };
    program.find_class(owner)
}

fn source_record_local(
    program: &JavaProgram,
    method: &JavaMethod,
    local: &str,
    member: &str,
    arity: usize,
) -> bool {
    let Some(class) = source_local_class(program, method, local) else {
        return false;
    };
    source_record_accessor(class, member, arity).is_some()
}

type InterfaceMember<'a> = (&'a JavaClass, usize, &'a JavaMethod);

fn extends_interface(program: &JavaProgram, child: &JavaClass, ancestor: &str) -> bool {
    let mut remaining: Vec<_> = child.interfaces().iter().map(String::as_str).collect();
    let mut seen = BTreeSet::new();
    while let Some(name) = remaining.pop() {
        if name == ancestor {
            return true;
        }
        if seen.insert(name) {
            if let Some(class) = program
                .find_class(name)
                .filter(|class| class.is_interface())
            {
                remaining.extend(class.interfaces().iter().map(String::as_str));
            }
        }
    }
    false
}

// Collect effective declarations by signature, not by declaration order.
// A more-specific abstract declaration shadows a default just as a more-
// specific default does. Unrelated maximal declarations are not discarded.
fn effective_interface_members<'a>(
    program: &'a JavaProgram,
    roots: impl IntoIterator<Item = &'a JavaClass>,
) -> Option<Vec<InterfaceMember<'a>>> {
    fn visit<'a>(
        program: &'a JavaProgram,
        interface: &'a JavaClass,
        seen: &mut BTreeSet<String>,
        active: &mut BTreeSet<String>,
        out: &mut Vec<InterfaceMember<'a>>,
    ) -> Option<()> {
        if !plain_source_interface(interface) || interface.supertype().is_some() {
            return None;
        }
        let name = interface.full_name();
        if active.contains(&name) {
            return None;
        }
        if !seen.insert(name.clone()) {
            return Some(());
        }
        active.insert(name.clone());
        if !interface.interfaces().is_empty() && !interface.type_parameters().is_empty() {
            return None;
        }
        for parent in interface.interfaces() {
            let declaration = program.find_class(parent)?;
            if !interface.interface_arguments(parent).is_empty()
                || !declaration.type_parameters().is_empty()
            {
                return None;
            }
            visit(program, declaration, seen, active, out)?;
        }
        out.extend(
            interface
                .methods()
                .iter()
                .enumerate()
                .map(|(i, m)| (interface, i, m)),
        );
        active.remove(&name);
        Some(())
    }
    let mut declarations = Vec::new();
    let mut seen = BTreeSet::new();
    let mut active = BTreeSet::new();
    for root in roots {
        visit(program, root, &mut seen, &mut active, &mut declarations)?;
    }
    let mut maximal = Vec::new();
    for &(owner, i, method) in &declarations {
        let mut shadowed = false;
        for &(other, _, candidate) in &declarations {
            if method.name() != candidate.name() || !same_parameters(method, candidate) {
                continue;
            }
            // This closed scalar subset has no covariant-return proof.
            if method.return_type() != candidate.return_type() {
                return None;
            }
            if extends_interface(program, other, &owner.full_name()) {
                shadowed = true;
            }
        }
        if !shadowed {
            maximal.push((owner, i, method));
        }
    }
    Some(maximal)
}

fn conflicting_default(members: &[InterfaceMember<'_>], method: &JavaMethod) -> bool {
    let mut matches = members
        .iter()
        .filter(|(_, _, other)| method.name() == other.name() && same_parameters(method, other));
    matches.next().is_some()
        && matches.next().is_some()
        && members.iter().any(|(_, _, other)| {
            other.is_default() && method.name() == other.name() && same_parameters(method, other)
        })
}

pub(crate) fn interface_member<'a>(
    program: &'a JavaProgram,
    interface: &'a JavaClass,
    member: &str,
    arity: usize,
) -> Option<InterfaceMember<'a>> {
    let members = effective_interface_members(program, [interface])?;
    let mut methods = members
        .iter()
        .copied()
        .filter(|(_, _, m)| m.name() == member && m.arity() == arity);
    let selected = methods.next()?;
    if conflicting_default(&members, selected.2)
        || methods.any(|(_, _, m)| !same_parameters(selected.2, m))
    {
        return None;
    }
    Some(selected)
}

pub(crate) fn selected_interface_method(
    program: &JavaProgram,
    class: &JavaClass,
    signature: &JavaMethod,
) -> Option<String> {
    if let Some((index, method)) = class
        .methods()
        .iter()
        .enumerate()
        .find(|(_, method)| override_matches(signature, method))
    {
        return Some(method_key(class, index, method));
    }
    // The runtime class can implement a more-specific interface than
    // the static receiver type. Do not select the static type's body.
    let roots = class
        .interfaces()
        .iter()
        .filter_map(|name| program.find_class(name))
        .filter(|interface| plain_source_interface(interface));
    let members = effective_interface_members(program, roots)?;
    if conflicting_default(&members, signature) {
        return None;
    }
    let (owner, index, method) = members.into_iter().find(|(_, _, candidate)| {
        candidate.is_default()
            && candidate.name() == signature.name()
            && same_parameters(signature, candidate)
    })?;
    Some(method_key(owner, index, method))
}

pub(crate) fn specialized_interface<'a>(
    program: &'a JavaProgram,
    ty: &JavaType,
) -> Option<&'a JavaClass> {
    let (name, args) = match ty {
        JavaType::Class(name) => (name, &[][..]),
        JavaType::Generic(name, args) => (name, args.as_slice()),
        _ => return None,
    };
    let class = program.find_class(name)?;
    let members = effective_interface_members(program, [class])?;
    if members
        .iter()
        .any(|(_, _, method)| conflicting_default(&members, method))
    {
        return None;
    }
    (class.type_parameters().len() == args.len() && args.iter().all(|arg|
        matches!(arg, JavaType::Class(name) if program.find_class(name).is_some()
            || matches!(name.as_str(), "String" | "java.lang.String" | "Object" | "java.lang.Object"
                | "Integer" | "java.lang.Integer" | "Long" | "java.lang.Long" | "Boolean" | "java.lang.Boolean"))))
        .then_some(class)
}

pub(crate) fn implements_specialization(
    program: &JavaProgram,
    class: &JavaClass,
    ty: &JavaType,
) -> bool {
    let (name, args) = match ty {
        JavaType::Class(name) => (name, &[][..]),
        JavaType::Generic(name, args) => (name, args.as_slice()),
        _ => return false,
    };
    class.interfaces().contains(name) && class.interface_arguments(name) == args
        || args.is_empty()
            && program.find_class(name).is_some_and(|interface| {
                interface.is_interface() && interface.type_parameters().is_empty()
            })
            && extends_interface(program, class, name)
}

fn local_interface_receiver<'a>(
    program: &'a JavaProgram,
    method: &simulator_parser::JavaMethod,
    local: &str,
) -> Option<&'a simulator_parser::JavaClass> {
    let declared = method
        .params()
        .iter()
        .find(|(_, name)| name == local)
        .map(|(ty, _)| ty)
        .or_else(|| {
            method.body()?.iter().find_map(|statement| match statement {
                S::VarDecl {
                    java_type, name, ..
                } if name == local => Some(java_type),
                _ => None,
            })
        })?;
    specialized_interface(program, declared)
}

fn builtin_resource_interface(program: &JavaProgram, interface: &str) -> bool {
    interface == "java.lang.AutoCloseable"
        || (interface == "AutoCloseable" && program.find_class(interface).is_none())
}

fn closed_resource_close(program: &JavaProgram, class: &JavaClass) -> Option<String> {
    if !class
        .interfaces()
        .iter()
        .any(|interface| builtin_resource_interface(program, interface))
    {
        return None;
    }
    let (index, method) = class
        .methods()
        .iter()
        .enumerate()
        .find(|(_, method)| method.name() == "close" && method.arity() == 0)?;
    if !method.is_public()
        || method.is_static()
        || *method.return_type() != JavaType::Void
        || method.body().is_none()
    {
        return None;
    }
    Some(method_key(class, index, method))
}

fn resource_blank_final(class: &JavaClass, field: &simulator_parser::ast::JavaField) -> bool {
    if field.has_initializer() {
        return false;
    }
    let mut all = class
        .methods()
        .iter()
        .filter(|method| method.is_constructor());
    let Some(constructor) = all.next() else {
        return false;
    };
    if all.next().is_some() {
        return false;
    }
    // A single simple constructor must definitely initialize this field.
    // The static checker also rejects later writes to the final field.
    constructor.body().is_some_and(|body| {
        let mut count = 0;
        for stmt in body {
            let S::Assign {
                target:
                    E::FieldAccess {
                        target,
                        field: assigned,
                    },
                value: E::Ident(parameter),
                is_compound: false,
            } = stmt
            else {
                return false;
            };
            if !matches!(target.as_ref(), E::Ident(name) if name == "this")
                || !constructor.params().iter().any(|(ty, name)| {
                    name == parameter
                        && class
                            .fields()
                            .iter()
                            .any(|f| f.name() == assigned && f.java_type() == ty)
                })
            {
                return false;
            }
            count += usize::from(assigned == field.name());
        }
        count == 1
    })
}

pub(crate) fn closed_resource_binding<'a>(
    program: &JavaProgram,
    resource: &'a S,
) -> Option<(&'a str, String)> {
    let S::VarDecl {
        name,
        java_type: JavaType::Class(owner),
        initializer:
            Some(E::New {
                java_type: JavaType::Class(allocation),
                ..
            }),
        ..
    } = resource
    else {
        return None;
    };
    if owner != allocation {
        return None;
    }
    let class = program.find_class(owner)?;
    Some((name, closed_resource_close(program, class)?))
}

/// Source layouts retain declaring owners. Primitive DTO fields follow the
/// business numeric domain and the existing Object carrier remains opaque.
pub(crate) fn source_layouts(
    program: &JavaProgram,
    receiver: Option<&crate::kernel_input::SourceReceiver>,
) -> Result<BTreeMap<String, BTreeMap<String, String>>, String> {
    let mut layouts = BTreeMap::new();
    for class in program.classes() {
        if !class.static_initializers().is_empty()
            && !receiver.is_some_and(|receiver| receiver.owner == class.full_name())
        {
            return Err(error("NORM-STATIC-INITIALIZER", &class.full_name()));
        }
        if admitted_exception_fields(program, class).is_some()
            || generic_product::prove(class).is_some()
        {
            continue;
        }
        if class.fields().is_empty() {
            if class.supertype().is_none()
                && (!class.is_interface()
                    || effective_interface_members(program, [class]).is_some())
            {
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
            if receiver.is_some_and(|receiver| {
                receiver.owner == class.full_name()
                    && receiver.assembly_fields.contains(field.name())
            }) {
                continue;
            }
            if crate::kernel_input::inert_singleton(program, class, field) {
                continue;
            }
            if source_ports::marked(class, field) {
                continue;
            }
            let state = entry_state(class, field);
            let wired = receiver.is_some_and(|receiver| {
                receiver
                    .dependency(&class.full_name(), field.name())
                    .is_some()
            });
            let annotation_ok =
                field.annotations().is_empty() || state || source_nullable_field(class, field);
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
                || field.is_final()
                    && !class.is_record()
                    && !wired
                    && !(closed_resource_close(program, class).is_some()
                        && resource_blank_final(class, field))
                || !initializer_ok
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
            // Every field name was checked before any of the admission paths above.
            fields.insert(field.name().into(), ty.into());
        }
        layouts.insert(class.full_name(), fields);
    }
    // Lexical short names are accepted only with one actual declared owner.
    let mut aliases = BTreeSet::new();
    for class in program
        .classes()
        .iter()
        .filter(|class| class.package().is_empty())
    {
        if !layouts.contains_key(&class.full_name()) {
            continue;
        }
        let short = class.simple_name().rsplit('.').next().unwrap();
        if !aliases.insert(short.to_string()) {
            return Err(error("NORM-AMBIGUOUS-OWNER", short));
        }
    }
    Ok(layouts)
}
const ENTRY_STATE: &str = "normalizer.intrinsics.N.EntryState";
const NULLABLE: &str = "normalizer.intrinsics.N.Nullable";
use simulator_parser::NORMALIZER_INTRINSICS_IMPORT as N_IMPORT;

/// Refuse counterfeit official definitions and ambiguous local bindings.
fn validate_official_namespace(program: &JavaProgram) -> Result<(), String> {
    let counterfeit_namespace = program.classes().iter().any(|class| {
        let name = class.full_name();
        name == N_IMPORT
            || name
                .strip_prefix(N_IMPORT)
                .is_some_and(|tail| tail.starts_with('.'))
    });
    if let Some(class) = program
        .classes()
        .iter()
        .find(|class| class.package() == "java" || class.package().starts_with("java."))
    {
        return Err(error("NORM-INTRINSIC-SHADOW", &class.full_name()));
    }
    if counterfeit_namespace
        || program.classes().iter().any(|class| {
            class.imports_normalizer_intrinsics()
                && crate::normalized_text::namespace_declared_in_unit(program, class)
        })
    {
        return Err(error("NORM-INTRINSIC-SHADOW", N_IMPORT));
    }
    for (qualified, alias) in [
        ("java.math.BigDecimal", "BigDecimal"),
        ("java.time.LocalDate", "LocalDate"),
    ] {
        if program
            .classes()
            .iter()
            .any(|class| class.full_name() == qualified)
            || program.classes().iter().any(|class| {
                class.simple_name() == alias
                    && program.classes().iter().any(|consumer| {
                        consumer.source_file() == class.source_file()
                            && consumer.imports().iter().any(|name| name == qualified)
                    })
            })
        {
            return Err(error("NORM-INTRINSIC-SHADOW", qualified));
        }
    }
    Ok(())
}

fn bare_annotation(annotations: &[AnnotationRef]) -> Option<&str> {
    let [annotation] = annotations else {
        return None;
    };
    (!annotation.has_arguments()).then(|| annotation.name())
}

fn official_marker_name(class: &simulator_parser::JavaClass, name: &str, qualified: &str) -> bool {
    name == qualified
        || (qualified.strip_prefix("normalizer.intrinsics.") == Some(name)
            && class.imports_normalizer_intrinsics())
}

fn official_marker(
    class: &simulator_parser::JavaClass,
    annotations: &[AnnotationRef],
    qualified: &str,
) -> bool {
    bare_annotation(annotations).is_some_and(|name| official_marker_name(class, name, qualified))
}

fn nullable_state_text_markers(
    class: &simulator_parser::JavaClass,
    field: &simulator_parser::JavaField,
) -> bool {
    if !matches!(field.java_type(), JavaType::Class(name) if name == "String" || name == "java.lang.String")
    {
        return false;
    }
    let [a, b] = field.annotations() else {
        return false;
    };
    let matches = |annotation: &AnnotationRef, marker| {
        !annotation.has_arguments() && official_marker_name(class, annotation.name(), marker)
    };
    (matches(a, ENTRY_STATE) && matches(b, NULLABLE))
        || (matches(a, NULLABLE) && matches(b, ENTRY_STATE))
}

pub(crate) fn entry_state(
    class: &simulator_parser::JavaClass,
    field: &simulator_parser::JavaField,
) -> bool {
    official_marker(class, field.annotations(), ENTRY_STATE)
        || nullable_state_text_markers(class, field)
}
pub(crate) fn source_nullable_parameter(
    class: &simulator_parser::JavaClass,
    method: &simulator_parser::JavaMethod,
    name: &str,
    ty: &JavaType,
) -> Result<bool, String> {
    match method.param_annotations(name) {
        [] => Ok(false),
        annotations @ [_]
            if (matches!(ty, JavaType::Class(_)) || numeric_array(ty))
                && official_marker(class, annotations, NULLABLE) =>
        {
            Ok(true)
        }
        [annotation] => Err(format!(
            "unsupported source parameter annotation {}",
            annotation.name()
        )),
        _ => Err("multiple source parameter annotations cannot define nullability".into()),
    }
}

pub(crate) fn source_nullable_field(
    class: &simulator_parser::JavaClass,
    field: &simulator_parser::JavaField,
) -> bool {
    matches!(field.java_type(), JavaType::Class(_))
        && (official_marker(class, field.annotations(), NULLABLE)
            || nullable_state_text_markers(class, field))
}

pub(crate) fn nullable_text_field(
    class: &simulator_parser::JavaClass,
    field: &simulator_parser::JavaField,
) -> bool {
    matches!(field.java_type(), JavaType::Class(name) if name == "String" || name == "java.lang.String")
        && source_nullable_field(class, field)
}
pub(crate) fn nullable_text_return(
    class: &simulator_parser::JavaClass,
    method: &simulator_parser::JavaMethod,
) -> bool {
    matches!(method.return_type(), JavaType::Class(name) if name == "String" || name == "java.lang.String")
        && official_marker(class, method.annotations(), NULLABLE)
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
    (
        "java.lang.IllegalArgumentException",
        Some(RUNTIME_EXCEPTION),
    ),
    (
        "java.lang.IndexOutOfBoundsException",
        Some(RUNTIME_EXCEPTION),
    ),
    (
        "java.lang.ArrayIndexOutOfBoundsException",
        Some("java.lang.IndexOutOfBoundsException"),
    ),
    (
        "java.lang.NegativeArraySizeException",
        Some(RUNTIME_EXCEPTION),
    ),
];

fn builtin_exception(name: &str) -> Option<(&'static str, Option<&'static str>)> {
    SOURCE_BUILTIN_EXCEPTIONS
        .iter()
        .copied()
        .find(|(qualified, _)| {
            *qualified == name || qualified.strip_prefix("java.lang.") == Some(name)
        })
}

pub(crate) fn builtin_exception_name(name: &str) -> Option<&'static str> {
    builtin_exception(name).map(|(qualified, _)| qualified)
}

pub(crate) fn exception_name(
    name: &str,
    owner: &str,
    parents: &BTreeMap<String, String>,
) -> Option<String> {
    if let Some(canonical) = builtin_exception_name(name) {
        return Some(canonical.into());
    }
    if parents.contains_key(name) {
        return Some(name.into());
    }
    let lexical = format!("{owner}.{name}");
    parents.contains_key(&lexical).then_some(lexical)
}
/// An admitted declaration shape, not evidence that a port returns an external payload.
fn admitted_exception_fields(
    program: &JavaProgram,
    class: &simulator_parser::JavaClass,
) -> Option<&'static str> {
    if !matches!(
        class.supertype(),
        Some("RuntimeException" | RUNTIME_EXCEPTION)
    ) {
        return None;
    }
    let [field] = class.fields() else {
        return None;
    };
    let field = Some(field).filter(|field| {
        field.name() == "payload"
            && !field.is_static()
            && !field.is_final()
            && !field.has_initializer()
    })?;
    match field.java_type() {
        JavaType::Class(name)
            if matches!(name.as_str(), "String" | "java.lang.String")
                && official_marker(class, field.annotations(), NULLABLE) =>
        {
            Some("text?")
        }
        ty if integral(ty) && field.annotations().is_empty() => Some("number"),
        JavaType::Class(name)
            if field.annotations().is_empty()
                && (name == "java.lang.Object"
                    || name == "Object" && program.find_class(name).is_none()) =>
        // A broad value field is not a declared external payload
        // contract. Preserve the exception class; never invent data.
        {
            Some("unit")
        }
        _ => None,
    }
}

pub(crate) fn source_exceptions(program: &JavaProgram) -> Result<BTreeMap<String, String>, String> {
    let mut parents = BTreeMap::new();
    for class in program.classes() {
        // The tiny builtin hierarchy is authoritative only when source cannot
        // shadow one of its names. Reject rather than guess lexical rebinding.
        let identity = class.full_name();
        let shadow_name = if class.package().is_empty() {
            class
                .simple_name()
                .rsplit('.')
                .next()
                .unwrap_or(class.simple_name())
        } else {
            &identity
        };
        if builtin_exception_name(shadow_name).is_some() {
            return Err(error("NORM-EXCEPTION-SHADOW", &class.full_name()));
        }
        if let Some(parent) = class.supertype() {
            if (!class.fields().is_empty() && admitted_exception_fields(program, class).is_none())
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
                .get(current)
                .map(String::as_str)
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
        let Some(next) = parents
            .get(current)
            .map(String::as_str)
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
fn closed_terminal_block(body: &[S]) -> bool {
    fn isolated(statement: &S) -> bool {
        match statement {
            S::VarDecl {
                initializer: Some(_),
                ..
            }
            | S::Throw(_)
            | S::Return(_) => true,
            S::Block(body) => body.iter().all(isolated),
            S::If {
                then_block,
                else_block,
                ..
            } => {
                then_block.iter().all(isolated)
                    && else_block
                        .as_ref()
                        .is_none_or(|other| other.iter().all(isolated))
            }
            _ => false,
        }
    }
    matches!(body.split_last(), Some((S::Return(_) | S::Throw(_), prefix)) if prefix.iter().all(isolated))
}

pub(crate) fn rethrows_caught_value(handler: &simulator_parser::ast::CatchClause) -> bool {
    matches!(handler.body(), [S::Throw(E::Ident(name))] if name == handler.var_name())
}

/// Syntactic no-argument throw shape. Admission binds its declared type before execution.
pub(crate) fn type_only_throw_name<'a>(statement: &'a S) -> Option<&'a str> {
    match statement {
        S::Throw(E::New {
            java_type: JavaType::Class(class),
            args,
        }) if args.is_empty() => Some(class),
        _ => None,
    }
}

// A continuing protected block may change heap cells, but never rebind an
// outer local: guarded stores are shared across normal and caught paths.
fn continuing_try_body(body: &[S]) -> bool {
    body.iter().all(|statement| match statement {
        S::Assign {
            target: E::ArrayAccess { .. },
            ..
        }
        | S::Throw(_)
        | S::DoOnceExit => true,
        S::If {
            then_block,
            else_block,
            ..
        } => {
            continuing_try_body(then_block) && else_block.as_deref().is_none_or(continuing_try_body)
        }
        _ => false,
    })
}
fn validate_protected_try(
    body: &[S],
    catches: &[simulator_parser::ast::CatchClause],
    finally: Option<&[S]>,
    id: &str,
) -> Result<(), String> {
    let closed_handlers = catches.iter().all(|handler| {
        closed_terminal_block(handler.body()) || finally.is_none() && handler.body().is_empty()
    });
    let terminal = closed_terminal_block(body);
    // In both forms the protected block changes only shared guarded
    // storage, not the local bindings that the handler or finally sees.
    let continuing = continuing_try_body(body)
        && ((catches.len() == 1 && finally.is_none()) || (catches.is_empty() && finally.is_some()));
    if closed_handlers && (!catches.is_empty() || finally.is_some()) && (terminal || continuing) {
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
    if class
        .methods()
        .iter()
        .filter(|m| m.name() == method.name())
        .count()
        > 1
    {
        format!("{base}@{index}")
    } else {
        base
    }
}

pub(crate) fn method_for_key<'a>(class: &'a JavaClass, key: &str) -> Option<&'a JavaMethod> {
    class
        .methods()
        .iter()
        .enumerate()
        .find_map(|(index, method)| (method_key(class, index, method) == key).then_some(method))
}

pub(crate) fn named_method_for_arity<'a>(
    class: &'a JavaClass,
    member: &str,
    arity: usize,
) -> Option<(String, &'a JavaMethod)> {
    let mut matches = class.methods().iter().enumerate().filter(|(_, method)| {
        !method.is_constructor() && method.name() == member && method.arity() == arity
    });
    let (index, method) = matches.next()?;
    matches
        .next()
        .is_none()
        .then(|| (method_key(class, index, method), method))
}

pub(crate) fn interface_super_qualifier(target: &E) -> Option<&str> {
    if let E::FieldAccess { target, field } = target {
        if field == "super" {
            if let E::Ident(name) = target.as_ref() {
                return Some(name);
            }
        }
    }
    None
}

/// Qualified super selects a declared default, never the runtime override.
pub(crate) fn interface_super_method<'a>(
    program: &'a JavaProgram,
    caller: &JavaClass,
    qualifier: &str,
    member: &str,
    arity: usize,
) -> Option<(String, &'a JavaMethod)> {
    let interface = program.find_class(qualifier)?;
    if !interface.is_interface()
        || !caller.interfaces().iter().any(|name| {
            program
                .find_class(name)
                .is_some_and(|direct| direct.full_name() == interface.full_name())
        })
    {
        return None;
    }
    if caller
        .interfaces()
        .iter()
        .filter_map(|name| program.find_class(name))
        .any(|other| {
            other.full_name() != interface.full_name()
                && extends_interface(program, other, &interface.full_name())
        })
    {
        return None;
    }
    let (owner, index, method) = interface_member(program, interface, member, arity)?;
    (method.is_default() && method.body().is_some())
        .then(|| (method_key(owner, index, method), method))
}

pub(crate) fn assembled_instance_method<'a>(
    caller: &JavaClass,
    class: &'a JavaClass,
    member: &str,
    arity: usize,
) -> Option<(String, &'a JavaMethod)> {
    named_method_for_arity(class, member, arity).filter(|(_, method)| {
        !method.is_static()
            && method.body().is_some()
            && instance_member_visible(caller, class, method)
    })
}

pub(crate) fn instance_member_visible(
    caller: &JavaClass,
    class: &JavaClass,
    method: &JavaMethod,
) -> bool {
    source_member_visible(caller, class, method.is_public(), method.is_private())
}

pub(crate) fn source_member_visible(
    caller: &JavaClass,
    class: &JavaClass,
    public: bool,
    private: bool,
) -> bool {
    if caller.package() != class.package() {
        return class.is_public_top_level() && public && !private;
    }
    !private || caller.simple_name().split('.').next() == class.simple_name().split('.').next()
}

pub(crate) fn constructor_for_arity(
    class: &JavaClass,
    arity: usize,
) -> Option<(String, &JavaMethod)> {
    let mut matches = class
        .methods()
        .iter()
        .enumerate()
        .filter(|(_, method)| method.is_constructor() && method.arity() == arity);
    let (index, method) = matches.next()?;
    matches
        .next()
        .is_none()
        .then(|| (method_key(class, index, method), method))
}

fn same_parameters(left: &JavaMethod, right: &JavaMethod) -> bool {
    left.params()
        .iter()
        .map(|(ty, _)| ty)
        .eq(right.params().iter().map(|(ty, _)| ty))
}

/// Validate every supplied method, including unused helpers.
pub(crate) fn validate_with_receiver(
    program: &JavaProgram,
    entry: &str,
    receiver: Option<&crate::kernel_input::SourceReceiver>,
    locator: Option<&str>,
) -> Result<SourceEvidence, String> {
    validate_official_namespace(program)?;
    // Refuse duplicate declarations before constructing class-keyed layout,
    // exception or member indexes. An alias collision between distinct
    // declarations is a different error from defining one owner twice.
    let mut classes = BTreeSet::new();
    for class in program.classes() {
        let name = class.full_name();
        if !classes.insert(name.clone()) {
            return Err(error("NORM-DUPLICATE-CLASS", &name));
        }
    }
    let exceptions = source_exceptions(program)?;
    let layouts = source_layouts(program, receiver)?;
    let ports = source_ports::declarations(program, &exceptions)?;
    if receiver.is_some() {
        external_polling::validate_entry(program, entry, locator.unwrap_or(entry), &ports)?;
    }
    let is_port_interface = |name: &str| ports.values().any(|port| port.interface == name);
    let supported = |ty: &JavaType| {
        scalar(ty)
            || native_decimal_type(ty)
            || exact_decimal_type(ty)
            || calendar_date_type(ty)
            || matches!(ty, JavaType::Array(item) if integral(item) || declared_reference(item, &layouts) || matches!(item.as_ref(), JavaType::Array(scalar) if integral(scalar)))
            || declared_reference(ty, &layouts)
            || matches!(ty, JavaType::Class(name) if is_port_interface(name))
            || specialized_interface(program, ty).is_some()
    };
    let mut methods = BTreeMap::new();
    let mut nullable_entry_parameters = BTreeSet::new();
    for c in program.classes() {
        let name = c.full_name();
        let valid_interfaces = if c.is_interface() {
            !plain_source_interface(c) && c.interfaces().is_empty()
                || effective_interface_members(program, [c]).is_some_and(|members| {
                    !members
                        .iter()
                        .any(|(_, _, method)| conflicting_default(&members, method))
                })
        } else {
            let ordinary = c
                .interfaces()
                .iter()
                .filter(|interface| !builtin_resource_interface(program, interface))
                .map(|interface| {
                    let args = c.interface_arguments(interface);
                    let ty = if args.is_empty() {
                        JavaType::Class(interface.clone())
                    } else {
                        JavaType::Generic(interface.clone(), args.to_vec())
                    };
                    specialized_interface(program, &ty)
                })
                .collect::<Option<Vec<_>>>();
            (!c.interfaces()
                .iter()
                .any(|name| builtin_resource_interface(program, name))
                || closed_resource_close(program, c).is_some())
                && ordinary
                    .and_then(|roots| effective_interface_members(program, roots))
                    .is_some_and(|members| {
                        members.iter().all(|(_, _, required)| {
                            let matching = c
                                .methods()
                                .iter()
                                .filter(|method| {
                                    method.name() == required.name()
                                        && same_parameters(method, required)
                                })
                                .collect::<Vec<_>>();
                            match matching.as_slice() {
                                [actual] => override_matches(required, actual),
                                [] => {
                                    required.is_default()
                                        && !conflicting_default(&members, required)
                                }
                                _ => false,
                            }
                        })
                    })
        };
        if !valid_interfaces
            || (!c.is_interface()
                && !c.type_parameters().is_empty()
                && !(c.fields().is_empty() && c.methods().is_empty())
                && generic_product::prove(c).is_none())
            || (c.is_interface()
                && !plain_source_interface(c)
                && (!is_port_interface(&name) || !c.fields().is_empty()))
            || !c.annotations().is_empty()
        {
            return Err(error("NORM-CLASS-SHAPE", &name));
        }
        if generic_product::prove(c).is_some() {
            continue;
        }
        for (index, m) in c.methods().iter().enumerate() {
            if c.methods()[..index].iter().any(|prior| {
                prior.name() == m.name()
                    && (match (prior.is_constructor(), m.is_constructor()) {
                        (true, true) => prior.arity() == m.arity(),
                        (false, false) => same_parameters(prior, m),
                        _ => true,
                    })
            }) {
                return Err(error("NORM-OVERLOAD", &format!("{name}.{}", m.name())));
            }
            if c.is_interface() && !m.is_default() {
                continue;
            }
            let id = method_key(c, index, m);
            methods.insert(id.clone(), (c, m));
            // A phantom method parameter header does not create a value
            // carrier. Do not let a type parameter shadow a resolvable
            // app or native type and obtain that type's semantics by name.
            if m.type_parameters().iter().any(|name| {
                let ty = JavaType::Class(name.clone());
                supported(&ty) || program.find_class(name).is_some()
            }) || (!m.annotations().is_empty() && !nullable_text_return(c, m))
                || (!m.is_constructor()
                    && !matches!(m.return_type(), JavaType::Void)
                    && !supported(m.return_type()))
                || m.params().iter().any(|(t, _)| !supported(t))
            {
                return Err(error("NORM-METHOD-SHAPE", &id));
            }
            for (ty, parameter) in m.params() {
                let nullable =
                    source_nullable_parameter(c, m, parameter, ty).map_err(|detail| {
                        crate::preprocess::required(
                            "NORM-SOURCE-ANNOTATION",
                            &format!("{id}.{parameter}"),
                            &detail,
                            "请按当前源码合同修改参数注解；不要删除来源语义。",
                        )
                    })?;
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
        statements(body, id, &mut calls, &mut catches)?;
        let entry_owner = entry.rsplit_once('.').map(|(owner, _)| owner);
        let closed_receiver = receiver.is_some() && entry_owner == Some(owner.full_name().as_str());
        static_types::validate(
            program,
            owner,
            m,
            id,
            body,
            &layouts,
            closed_receiver,
            receiver,
        )?;
        let mut targets = Vec::new();
        for (call, arity) in calls {
            let (target_owner, target_member) = match call {
                SourceCall::Resolved(key) => {
                    targets.push(key);
                    continue;
                }
                SourceCall::Method { owner, member } => (owner, member),
                SourceCall::Implicit { member } => {
                    if owner.is_interface() && !m.is_static() {
                        interface_member(program, owner, &member, arity).ok_or_else(|| {
                            error("NORM-UNBOUND-CALL", &format!("{id} -> {member}/{arity}"))
                        })?;
                        continue;
                    }
                    (owner.full_name(), member)
                }

                SourceCall::InterfaceSuper { qualifier, member } => {
                    let target = (!m.is_static()).then(|| interface_super_method(program, owner, &qualifier, &member, arity))
                        .flatten().map(|(key, _)| key).filter(|key| methods.contains_key(key))
                        .ok_or_else(|| crate::preprocess::required(
                            "NORM-SOURCE-TYPE", id,
                            "qualified interface super requires a direct implemented interface with a visible default body in an instance method",
                            "保留实际的直接父接口和默认方法，不把它改成普通虚调用。",
                        ))?;
                    targets.push(target);
                    continue;
                }
                SourceCall::StaticReceiver {
                    owner: root,
                    field,
                    member,
                } => {
                    let target = program
                        .find_class(&root)
                        .and_then(|root| {
                            crate::kernel_input::inert_instance_method(
                                program, owner, root, &field, &member, arity,
                            )
                        })
                        .ok_or_else(|| {
                            error(
                                "NORM-UNBOUND-CALL",
                                &format!("{id} -> {root}.{field}.{member}/{arity}"),
                            )
                        })?;
                    targets.push(target.2);
                    continue;
                }
                SourceCall::Constructor(owner) => {
                    let class = program
                        .find_class(&owner)
                        .ok_or_else(|| error("NORM-UNBOUND-CALL", &owner))?;
                    if class.is_record() {
                        if arity != class.fields().len() {
                            return Err(error(
                                "NORM-RECORD-CONSTRUCTOR",
                                &format!("{id} -> {owner}"),
                            ));
                        }
                        continue;
                    }
                    if !class.methods().iter().any(|m| m.is_constructor()) && arity == 0 {
                        continue;
                    }
                    let (target, _) = constructor_for_arity(class, arity).ok_or_else(|| {
                        error(
                            "NORM-UNBOUND-CALL",
                            &format!("{id} -> {owner} constructor/{arity}"),
                        )
                    })?;
                    targets.push(target);
                    continue;
                }
            };
            let target = format!("{target_owner}.{target_member}");
            if ports
                .get(&format!("{}.{target}", owner.full_name()))
                .is_some_and(|port| port.params.len() == arity)
            {
                continue;
            }
            if !m
                .params()
                .iter()
                .any(|(_, parameter)| parameter == &target_owner)
                && (crate::normalized_text::source_intrinsic(program, owner, &target, arity)
                    .is_some()
                    || crate::normalized_decimal::decimal_intrinsic(program, owner, &target)
                        .is_some()
                    || crate::normalized_text::clock_intrinsic(program, owner, &target, arity))
            {
                continue;
            }
            // Calls already have their declaring name and member. Preserve
            // the historical first-dot lookup for flattened nested owners.
            let (local, member): (&str, Cow<'_, str>) = match target_owner.split_once('.') {
                Some((local, nested)) => (local, Cow::Owned(format!("{nested}.{target_member}"))),
                None => (target_owner.as_str(), Cow::Borrowed(target_member.as_str())),
            };
            if source_record_local(program, m, local, member.as_ref(), arity) {
                continue;
            }
            if let Some(interface) = local_interface_receiver(program, m, local).or_else(|| {
                (owner.is_interface() && !m.is_static() && local == "this").then_some(owner)
            }) {
                if interface_member(program, interface, member.as_ref(), arity).is_none() {
                    return Err(error(
                        "NORM-UNBOUND-CALL",
                        &format!("{id} -> {target}/{arity}"),
                    ));
                }
                // Dynamic edges are not the set of all implementations
                // present in this source. Runtime checks the selected
                // allocation, and the active stack still rejects cycles.
                continue;
            }
            if let Some((key, _)) = source_local_class(program, m, local)
                .and_then(|class| named_method_for_arity(class, member.as_ref(), arity))
                .filter(|(_, method)| !method.is_static() && method.body().is_some())
            {
                targets.push(key);
                continue;
            }
            if !m
                .params()
                .iter()
                .any(|(_, parameter)| parameter == &target_owner)
            {
                if let Some((key, _)) = receiver
                    .and_then(|r| r.target_class(&owner.full_name(), &target_owner))
                    .and_then(|name| program.find_class(name))
                    .and_then(|class| {
                        assembled_instance_method(owner, class, &target_member, arity)
                    })
                {
                    targets.push(key);
                    continue;
                }
                if let Some((_, _, key)) = crate::kernel_input::inert_instance_method(
                    program,
                    owner,
                    owner,
                    &target_owner,
                    &target_member,
                    arity,
                ) {
                    targets.push(key);
                    continue;
                }
            }
            let declaring = program.find_class(&target_owner);
            if declaring.is_some_and(|class| {
                class
                    .methods()
                    .iter()
                    .filter(|method| {
                        !method.is_constructor()
                            && method.name() == target_member
                            && method.arity() == arity
                    })
                    .count()
                    > 1
            }) {
                return Err(crate::preprocess::required(
                    "NORM-OVERLOADED-CALL",
                    id,
                    &format!("no checked call-site dispatch for {target}/{arity}"),
                    "同参数个数的重载需要解析实际参数类型；不能靠声明顺序猜测目标。",
                ));
            }
            let resolved = declaring
                .and_then(|class| named_method_for_arity(class, &target_member, arity))
                .map(|(key, _)| key)
                .filter(|key| methods.contains_key(key))
                .ok_or_else(|| error("NORM-UNBOUND-CALL", &format!("{id} -> {target}/{arity}")))?;
            if resolved != *id || affine_recursion::prove(m).is_none() {
                targets.push(resolved);
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
    Ok(SourceEvidence {
        layouts,
        exception_parents: exceptions,
        exception_bindings,
        ports,
        nullable_entry_parameters,
    })
}

enum SourceCall {
    Resolved(String),
    Method {
        owner: String,
        member: String,
    },
    InterfaceSuper {
        qualifier: String,
        member: String,
    },
    StaticReceiver {
        owner: String,
        field: String,
        member: String,
    },
    Implicit {
        member: String,
    },
    Constructor(String),
}

/// Direct Java payload is the explicit String constructor argument, not an
/// observed exception object or a guessed call to getMessage().
pub(crate) enum NativeThrowMessage<'a> {
    Absent,
    Null,
    Text(&'a str),
    TextUnits(&'a [u16]),
}

pub(crate) fn native_text_exception_type(class: &str) -> bool {
    matches!(
        class,
        RUNTIME_EXCEPTION | "java.lang.IllegalArgumentException"
    )
}

pub(crate) fn native_literal_throw(e: &E) -> Option<(&'static str, NativeThrowMessage<'_>)> {
    let E::New {
        java_type: JavaType::Class(class),
        args,
    } = e
    else {
        return None;
    };
    let class = builtin_exception_name(class)?;
    // Only these certified native constructors are lowered here. An
    // application exception with the same simple name is not a builtin.
    if !native_text_exception_type(class) {
        return None;
    }
    let message = match args.as_slice() {
        [] => NativeThrowMessage::Absent,
        [E::Lit(JavaLit::Str(text))] => NativeThrowMessage::Text(text),
        [E::Lit(JavaLit::StrUnits(units))] => NativeThrowMessage::TextUnits(units),
        [E::Cast {
            target_type: JavaType::Class(ty),
            operand,
        }] if ty == "java.lang.String" && matches!(operand.as_ref(), E::Lit(JavaLit::Null)) => {
            NativeThrowMessage::Null
        }
        _ => return None,
    };
    Some((class, message))
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
        let owner = at
            .rsplit_once('.')
            .expect("source method has a declaring class")
            .0;
        let name = exception_name(raw, owner, self.parents).ok_or_else(|| {
            let problem = if self.program.find_class(raw).is_some()
                || self.program.find_class(&format!("{owner}.{raw}")).is_some()
            {
                format!("catch class {raw} is not a Throwable")
            } else {
                format!("undeclared catch class {raw}")
            };
            crate::preprocess::required(
                "NORM-CATCH-TYPE",
                at,
                &problem,
                "请在源码入口声明可解析的异常类；不要把非法 catch 当成不匹配。",
            )
        })?;
        self.resolved.insert(raw.into(), name.clone());
        Ok(name)
    }

    fn bind_type_only_throw(&mut self, at: &str, raw: &str) -> Result<(), String> {
        let owner = at.rsplit_once('.').map_or("", |(owner, _)| owner);
        let name = exception_name(raw, owner, self.parents)
            .filter(|name| {
                builtin_exception_name(name).is_some()
                    || self
                        .program
                        .find_class(name)
                        .is_some_and(|class| class.fields().is_empty())
            })
            .ok_or_else(|| {
                error(
                    "NORM-EXPLICIT-THROW",
                    &format!("{at}: {raw} requires a declared type-only exception"),
                )
            })?;
        self.resolved.insert(raw.into(), name);
        Ok(())
    }
}

fn statements(
    body: &[S],
    at: &str,
    calls: &mut Vec<(SourceCall, usize)>,
    catch_admission: &mut CatchAdmission<'_>,
) -> Result<(), String> {
    for stmt in body {
        match stmt {
            S::LocalIr(
                normalizer_ir::local::LocalIr::IntegerRange(_)
                | normalizer_ir::local::LocalIr::MemberChoice { .. },
            ) => {}
            S::For { .. } if fixed_for::RectangularCounter::from_for(stmt).is_some() => {}
            S::For { .. } if fixed_for::BoundedControlFor::from_for(stmt).is_some() => {}
            S::LoopBreak | S::LoopContinue => return Err(error("NORM-LOOP", at)),
            S::SwitchBreak | S::DoOnceExit => {}
            S::For { cond, body, .. } if fixed_for::FixedFieldFor::from_for(stmt).is_some() => {
                // The proof admits just one literal store and a unit update.
                // The typed source pass checks both field lvalues.
                expr(cond.as_ref().unwrap(), at, calls)?;
                statements(body, at, calls, catch_admission)?;
            }
            S::For {
                init,
                cond,
                update,
                body,
            } if fixed_for::FixedFor::from_header(
                init.as_deref(),
                cond.as_ref(),
                update.as_ref(),
            )
            .is_some() =>
            {
                if let Some(S::Block(declarations)) = init.as_deref() {
                    statements(declarations, at, calls, catch_admission)?;
                }
                statements(body, at, calls, catch_admission)?;
            }
            S::Try {
                resource,
                body,
                catches,
                finally_block,
            } => {
                if let Some(resource) = resource {
                    let (_, close) = closed_resource_binding(catch_admission.program, resource)
                        .ok_or_else(|| error("NORM-RESOURCE-CLOSURE", at))?;
                    statements(
                        std::slice::from_ref(resource.as_ref()),
                        at,
                        calls,
                        catch_admission,
                    )?;
                    calls.push((SourceCall::Resolved(close), 0));
                    if catches.is_empty()
                        && !closed_terminal_block(body)
                        && !continuing_try_body(body)
                    {
                        return Err(error("NORM-CATCH-BODY", at));
                    }
                }
                if resource.is_none() || !catches.is_empty() {
                    validate_protected_try(body, catches, finally_block.as_deref(), at)?;
                }
                statements(body, at, calls, catch_admission)?;
                let mut prior_types: Vec<String> = Vec::new();
                for handler in catches {
                    let mut alternatives: Vec<String> = Vec::new();
                    for name in handler.exception_classes() {
                        let class = catch_admission.bind(at, name)?;
                        if let Some(other) = alternatives.iter().find(|other| {
                            subtype(&class, other, catch_admission.parents)
                                || subtype(other, &class, catch_admission.parents)
                        }) {
                            return Err(crate::preprocess::required(
                                "NORM-CATCH-TYPE",
                                at,
                                &format!("multi-catch alternatives {class} and {other} overlap"),
                                "并列的异常类型不能相同，也不能互为父子类。",
                            ));
                        }
                        if let Some(prior) = prior_types
                            .iter()
                            .find(|prior| subtype(&class, prior, catch_admission.parents))
                        {
                            return Err(crate::preprocess::required(
                                "NORM-CATCH-ORDER",
                                at,
                                &format!("catch class {class} is already caught by {prior}"),
                                "把更具体的异常处理器放在父类之前，不要删除处理分支。",
                            ));
                        }
                        alternatives.push(class);
                    }
                    prior_types.extend(alternatives);
                    if !rethrows_caught_value(handler) {
                        statements(handler.body(), at, calls, catch_admission)?;
                    }
                }
                if let Some(finally) = finally_block {
                    statements(finally, at, calls, catch_admission)?;
                }
            }
            S::LocalIr(
                normalizer_ir::local::LocalIr::LocalMapEmpty { .. }
                | normalizer_ir::local::LocalIr::LocalMapPut { .. }
                | normalizer_ir::local::LocalIr::LocalMapGet { .. }
                | normalizer_ir::local::LocalIr::ProjectedUnionCount { .. }
                | normalizer_ir::local::LocalIr::SqlWrite(_)
                | normalizer_ir::local::LocalIr::Relation(_)
                | normalizer_ir::local::LocalIr::Decimal(_),
            ) => {}
            S::VarDecl {
                java_type,
                initializer: Some(init),
                ..
            } if host_collection_allocation(java_type, init) => {}
            S::VarDecl {
                java_type,
                initializer: Some(init),
                ..
            } if generic_product::instantiated(catch_admission.program, java_type).is_some() => {
                expr(init, at, calls)?
            }
            S::VarDecl {
                java_type,
                initializer: None,
                ..
            } if scalar(java_type)
                || numeric_array(java_type)
                || record_array_component(java_type).is_some()
                || matches!(java_type, JavaType::Generic(_, _) | JavaType::Class(_)) => {}
            S::VarDecl {
                java_type,
                initializer: Some(e @ E::Ident(_)),
                ..
            } if record_array_component(java_type).is_some() => expr(e, at, calls)?,
            S::VarDecl {
                java_type,
                initializer: Some(e),
                ..
            } if scalar(java_type)
                || native_decimal_type(java_type)
                || matches!(java_type, JavaType::Class(_) | JavaType::Generic(..))
                || numeric_array(java_type) =>
            {
                expr(e, at, calls)?
            }
            S::Assign {
                target: E::ArrayAccess { target, index },
                value,
                ..
            } if constant_array_address(target, index) => {
                expr(target, at, calls)?;
                expr(index, at, calls)?;
                expr(value, at, calls)?;
            }
            S::Assign {
                target: E::FieldAccess { target, .. },
                value,
                ..
            } => {
                expr(target, at, calls)?;
                expr(value, at, calls)?;
            }
            S::Assign {
                target: E::Ident(_),
                value,
                ..
            } => expr(value, at, calls)?,
            S::Throw(E::Lit(JavaLit::Null)) => (),
            S::Throw(e) if native_literal_throw(e).is_some() => (),
            S::Throw(e) => {
                if let Some(class) = type_only_throw_name(stmt) {
                    catch_admission.bind_type_only_throw(at, class)?;
                } else {
                    expr(e, at, calls)?;
                    return Err(error("NORM-EXPLICIT-THROW", at));
                }
            }
            S::ObjectSuperConstructor => (),
            S::ThisConstructor(args) => {
                let owner = at.rsplit_once('.').expect("constructor key has an owner").0;
                for a in args {
                    expr(a, at, calls)?;
                }
                calls.push((SourceCall::Constructor(owner.into()), args.len()));
            }
            S::Return(Some(e)) | S::ExprStmt(e) => expr(e, at, calls)?,
            S::Return(None) => (),
            S::Block(b) => statements(b, at, calls, catch_admission)?,
            S::If {
                cond,
                then_block,
                else_block,
            } => {
                expr(cond, at, calls)?;
                statements(then_block, at, calls, catch_admission)?;
                if let Some(b) = else_block {
                    statements(b, at, calls, catch_admission)?;
                }
            }
            S::Switch {
                selector,
                cases,
                default,
                ..
            } => {
                expr(selector, at, calls)?;
                for (_, body) in cases {
                    statements(body, at, calls, catch_admission)?;
                }
                statements(default, at, calls, catch_admission)?;
            }
            S::ForEach {
                var_type,
                var_name,
                iterable,
                body,
            } if integral(var_type) && array_sum(var_name, body).is_some() => {
                expr(iterable, at, calls)?;
            }
            S::DoWhile {
                cond: E::Lit(JavaLit::Bool(false)),
                body,
            } => {
                statements(body, at, calls, catch_admission)?;
            }
            S::While { .. } | S::DoWhile { .. } | S::For { .. } | S::ForEach { .. } => {
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
        && (matches!(index, E::Lit(JavaLit::Int(_)))
            || matches!(index, E::UnaryOp { op: U::Neg, operand }
                if matches!(operand.as_ref(), E::Lit(JavaLit::Int(_)))))
}

// Index eligibility is checked by the static type pass, not its syntax.
fn source_array_read(target: &E) -> bool {
    matches!(target, E::Ident(_)) || inline_int_array_initializer(target)
}

// Match one full accumulator update and reject every other loop shape at
// one boundary. One closed integer addition to the element is supported.
pub(crate) fn array_sum<'a>(item: &str, body: &'a [S]) -> Option<(&'a str, i64)> {
    let [S::Assign {
        target: E::Ident(total),
        value: E::BinOp {
            op: B::Add,
            left,
            right,
        },
        ..
    }] = body
    else {
        return None;
    };
    if item == total.as_str() {
        return None;
    }
    let names =
        |expression: &E, expected: &str| matches!(expression, E::Ident(name) if name == expected);
    let element = if names(left, total) {
        right
    } else if names(right, total) {
        left
    } else {
        return None;
    };
    let offset = match element.as_ref() {
        E::Ident(name) if name == item => Some(0),
        E::BinOp {
            op: B::Add,
            left,
            right,
        } if names(left, item) => integer_token(right),
        E::BinOp {
            op: B::Add,
            left,
            right,
        } if names(right, item) => integer_token(left),
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

pub(crate) fn int_array_component(ty: &JavaType) -> bool {
    match ty {
        JavaType::Array(element) => int_array_component(element),
        _ => java_int(ty),
    }
}

fn inline_int_array_initializer(e: &E) -> bool {
    matches!(e, E::ArrayInitializer { element_type, .. } if java_int(element_type))
}

fn expr(e: &E, at: &str, calls: &mut Vec<(SourceCall, usize)>) -> Result<(), String> {
    match e {
        E::Ident(_)
        | E::Lit(JavaLit::Bool(_) | JavaLit::Str(_) | JavaLit::StrUnits(_) | JavaLit::Null) => (),
        E::Lit(JavaLit::Int(_) | JavaLit::Long(_) | JavaLit::Char(_) | JavaLit::Decimal { .. }) => {
            ()
        }
        E::Cast {
            target_type,
            operand,
        } if matches!(target_type, JavaType::Primitive(n) if n == "long") => {
            expr(operand, at, calls)?
        }
        E::BinOp {
            op:
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
                | B::Div
                | B::Mod,
            left,
            right,
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
        E::New {
            java_type: JavaType::Generic(_, _),
            args,
        } => {
            // The typed pass binds this source constructor with its actual
            // type arguments; no host or raw generic is dispatched here.
            for arg in args {
                expr(arg, at, calls)?;
            }
        }
        E::ArrayInitializer {
            element_type,
            elements,
        } if int_array_component(element_type) => {
            for element in elements {
                expr(element, at, calls)?;
            }
        }
        E::ArrayAllocation {
            element_type,
            length,
        } if java_int(element_type) => {
            expr(length, at, calls)?;
        }
        E::FieldAccess { target, .. } => expr(target, at, calls)?,
        E::ArrayAccess { target, index } if source_array_read(target) => {
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
                        let E::Ident(owner) = target.as_ref() else {
                            return Err(error("NORM-CALL-TARGET", at));
                        };
                        if field == "super" {
                            SourceCall::InterfaceSuper {
                                qualifier: owner.clone(),
                                member: method.clone(),
                            }
                        } else if owner == "this" {
                            SourceCall::Method {
                                owner: field.clone(),
                                member: method.clone(),
                            }
                        } else {
                            SourceCall::StaticReceiver {
                                owner: owner.clone(),
                                field: field.clone(),
                                member: method.clone(),
                            }
                        }
                    }
                    t => SourceCall::Method {
                        owner: source_ports::receiver(t)
                            .ok_or_else(|| error("NORM-CALL-TARGET", at))?
                            .into(),
                        member: method.clone(),
                    },
                },
                None => SourceCall::Implicit {
                    member: method.clone(),
                },
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
