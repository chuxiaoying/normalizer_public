//! Resolve public source entry identity before the execution key is selected.
use super::Result;
use simulator_parser::{JavaClass, JavaExpr, JavaField, JavaLit, JavaProgram, JavaStatement, JavaType};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

#[derive(Debug)]
pub(super) struct SourceEntry {
    pub(super) locator: String,
    pub(super) canonical_function: String,
    pub(super) project_root: PathBuf,
    pub(super) target: String,
    pub(super) receiver: Option<SourceReceiver>,
}

#[derive(Debug)]
pub(crate) struct SourceReceiver {
    pub(crate) owner: String,
    pub(crate) field: String,
    pub(crate) assembly_fields: BTreeSet<String>,
    // The key is the real static receiver field and its injected member,
    // not the service class name (two roots may have the same class).
    links: BTreeMap<String, BTreeMap<String, AssemblyLink>>,
    pub(crate) startup_setters: BTreeSet<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct AssemblyLink {
    pub(crate) receiver_class: String,
    pub(crate) object: AssemblyObject,
    pub(crate) target_class: String,
}

/// One object per real static field or allocation at a constructor argument
/// position. Equal types do not make two allocation sites the same object.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum AssemblyObject {
    StaticField(String),
    ConstructorArgument(usize),
}

#[derive(Clone, Copy)]
enum AssemblyArgument<'a> {
    Literal,
    Object(&'a JavaClass),
}

/// The implicit constructor of an ordinary class has no parameters. A
/// record instead has its canonical component parameters, even though the
/// source parser does not synthesize a constructor declaration.
fn has_zero_argument_implicit_constructor(class: &JavaClass) -> bool {
    !class.is_record() || class.fields().is_empty()
}

impl SourceReceiver {
    pub(crate) fn dependency(&self, owner: &str, member: &str) -> Option<&AssemblyLink> {
        self.links.get(&self.field)?.get(member)
            .filter(|link| link.receiver_class == owner)
    }

    pub(crate) fn target_class(&self, owner: &str, member: &str) -> Option<&str> {
        self.links.values().filter_map(|members| members.get(member)).find(|link| link.receiver_class == owner)
            .map(|link| link.target_class.as_str())
    }

    fn certify_setter(
        &mut self, program: &JavaProgram, root: &JavaClass,
        initialized: &BTreeMap<&str, &JavaType>, receiver: &JavaExpr, method: &str, args: &[JavaExpr],
    ) -> Result<()> {
        let (JavaExpr::Ident(receiver), [JavaExpr::Ident(operand)]) = (receiver, args) else {
            return Err("static assembly setter needs two previously assembled named objects".into());
        };
        let Some(JavaType::Class(owner)) = initialized.get(receiver.as_str()).copied() else {
            return Err("static assembly setter receiver used before its declaration".into());
        };
        let Some(JavaType::Class(target)) = initialized.get(operand.as_str()).copied() else {
            return Err("static assembly setter dependency used before its declaration".into());
        };
        let holder = program.find_class(owner).ok_or("missing static assembly setter owner")?;
        let dependency = program.find_class(target).ok_or("missing statically assembled dependency")?;
        let (key, setter) = crate::normalized::named_method_for_arity(holder, method, 1)
            .filter(|(_, setter)| !setter.is_static()
                && crate::normalized::instance_member_visible(root, holder, setter)
                && setter.throws().is_empty() && setter.annotations().is_empty()
                && crate::normalized::plain_source_data_method(holder, setter))
            .ok_or("static assembly setter must resolve to one visible direct field assignment")?;
        let Some([JavaStatement::Assign { target: field, .. }]) = setter.body() else {
            return Err("static assembly setter is not a direct field assignment".into());
        };
        let name = match field {
            JavaExpr::Ident(name) => name,
            JavaExpr::FieldAccess { field, .. } => field,
            _ => return Err("static assembly setter does not set a source field".into()),
        };
        let assigned = holder.fields().iter().find(|field| !field.is_static() && !field.is_final()
            && field.annotations().is_empty() && field.name() == name)
            .ok_or("static assembly setter needs a plain internal object field")?;
        if initialized.get(operand.as_str()).copied() != Some(assigned.java_type())
            || !dependency.fields().is_empty()
        {
            return Err("static assembly setter needs an exact stateless source dependency".into());
        }
        self.links.entry(receiver.clone()).or_default().insert(name.clone(), AssemblyLink {
            receiver_class: holder.full_name(), object: AssemblyObject::StaticField(operand.clone()),
            target_class: dependency.full_name(),
        });
        self.startup_setters.insert(key);
        Ok(())
    }

    /// Establish the identity of a pre-existing receiver, not a fresh Java
    /// allocation in the analyzed invocation. Only straight-line, non-throwing
    /// assembly is admitted by this route.
    fn certify(
        program: &JavaProgram, caller: &JavaClass, class: &JavaClass, args: &[JavaExpr],
    ) -> Result<BTreeMap<String, AssemblyLink>> {
        if class.is_interface() || class.supertype().is_some() || !class.static_initializers().is_empty() {
            return Err("unsupported receiver assembly class".into());
        }
        let constructor = if class.methods().iter().any(|method| method.is_constructor()) {
            let (_, selected) = crate::normalized::constructor_for_arity(class, args.len())
                .ok_or("receiver assembly requires a unique constructor of the invoked arity")?;
            if !crate::normalized::instance_member_visible(caller, class, selected) {
                return Err("receiver constructor is not accessible from the assembly class".into());
            }
            Some(selected)
        } else {
            if !has_zero_argument_implicit_constructor(class) {
                return Err("record components require a canonical constructor; not inert receiver assembly".into());
            }
            None
        };
        let params = constructor.map(|m| m.params()).unwrap_or_default();
        if params.len() != args.len() {
            return Err("unsupported or mismatched receiver constructor arguments".into());
        }
        let checked_args = params.iter().zip(args)
            .map(|((ty, _), arg)| assembly_argument(program, ty, arg))
            .collect::<Option<Vec<_>>>().ok_or("unsupported or mismatched receiver constructor arguments")?;
        let mut attached_ports = BTreeSet::new();
        let mut dependencies = BTreeMap::new();
        if let Some(constructor) = constructor {
            if !constructor.throws().is_empty() {
                return Err("receiver constructor may throw".into());
            }
            let mut seen = BTreeSet::new();
            for stmt in constructor.body().ok_or("missing receiver constructor body")? {
                let JavaStatement::Assign { target: JavaExpr::FieldAccess { target, field }, value: JavaExpr::Ident(parameter), is_compound: false } = stmt
                    else { return Err("only direct receiver constructor assignments are supported".into()); };
                if !matches!(target.as_ref(), JavaExpr::Ident(name) if name == "this") || !seen.insert(field.as_str()) {
                    return Err("ambiguous receiver field initialization".into());
                }
                let declaration = class.fields().iter().find(|f| f.name() == field && !f.is_static())
                    .ok_or("missing receiver field declaration")?;
                let index = params.iter().position(|(ty, name)| name == parameter && ty == declaration.java_type())
                    .ok_or("receiver constructor must assign a parameter of the declared field type")?;
                if crate::normalized::source_ports::marked(class, declaration) {
                    if !matches!(checked_args[index], AssemblyArgument::Object(_)) {
                        return Err("external receiver field must be bound to a declared port object".into());
                    }
                    attached_ports.insert(field.as_str());
                } else if !crate::normalized::entry_state(class, declaration) {
                    let AssemblyArgument::Object(dependency) = checked_args[index] else {
                        return Err("receiver constructor writes a field outside its entry contract".into());
                    };
                    if !declaration.annotations().is_empty() {
                        return Err("receiver constructor writes a field outside its entry contract".into());
                    }
                    dependencies.insert(field.to_owned(), AssemblyLink {
                        receiver_class: class.full_name(),
                        object: AssemblyObject::ConstructorArgument(index),
                        target_class: dependency.full_name(),
                    });
                }
            }
        }
        if class.fields().iter().any(|f| !f.is_static()
            && crate::normalized::source_ports::marked(class, f)
            && !attached_ports.contains(f.name()))
        {
            return Err("external receiver field must have a proven binding".into());
        }
        // Executable uses of plain receiver fields are rejected by the
        // scope-aware source gate; an unused declaration need not become E.
        Ok(dependencies)
    }
    fn certify_static_assembly(&mut self, program: &JavaProgram, root: &JavaClass) -> Result<()> {
        self.assembly_fields.insert(self.field.clone());
        if root.static_initializers().is_empty() { return Ok(()); }
        if root.is_interface() || root.supertype().is_some() {
            return Err("unsupported assembly host".into());
        }
        let mut initialized = std::collections::BTreeMap::new();
        let mut next = 0;
        for (fields_before, block) in root.static_initializers() {
            for field in &root.fields()[next..*fields_before] {
                if field.name() != self.field {
                    if !field.is_static() || !field.is_final() || !field.annotations().is_empty()
                        || !field.initializer().is_some_and(|arg| assembly_allocation(program, field.java_type(), arg))
                    {
                        return Err("unverified auxiliary static assembly field".into());
                    }
                }
                initialized.insert(field.name(), field.java_type());
                self.assembly_fields.insert(field.name().into());
            }
            next = *fields_before;
            for stmt in block {
                if let JavaStatement::ExprStmt(JavaExpr::MethodCall { target: Some(target), method, args }) = stmt {
                    self.certify_setter(program, root, &initialized, target, method, args)?;
                    continue;
                }
                let JavaStatement::Assign { target: JavaExpr::FieldAccess { target, field }, value, is_compound: false } = stmt
                else { return Err("static assembly only supports direct field assignments".into()); };
                let JavaExpr::Ident(receiver) = target.as_ref() else {
                    return Err("static assembly write must target a freshly assembled root".into());
                };
                let Some(JavaType::Class(owner)) = initialized.get(receiver.as_str()).copied() else {
                    return Err("static assembly receiver used before its declaration".into());
                };
                let declaration = program.find_class(owner).ok_or("missing static assembly receiver type")?;
                let member = declaration.fields().iter().find(|f| !f.is_static() && !f.is_final() && f.name() == field)
                    .ok_or("invalid static assembly field assignment")?;
                if !crate::normalized::source_member_visible(root, declaration, member.is_public(), member.is_private()) {
                    return Err("static assembly cannot write a field that is not visible from the assembly class".into());
                }
                if receiver == &self.field && !crate::normalized::entry_state(declaration, member) {
                    return Err("static assembly may only write declared state on the entry receiver".into());
                }
                let assigned_type = member.java_type();
                let valid = match value {
                    JavaExpr::Ident(name) => initialized.get(name.as_str()).copied() == Some(assigned_type),
                    JavaExpr::ArrayAllocation { element_type, length }
                        if matches!(assigned_type, JavaType::Array(item) if item.as_ref() == element_type)
                            && matches!(element_type, JavaType::Primitive(name) if name == "int") =>
                        matches!(length.as_ref(), JavaExpr::Lit(JavaLit::Int(n)) if *n >= 0),
                    _ => assembly_argument(program, assigned_type, value).is_some()
                        || assembly_allocation(program, assigned_type, value),
                };
                if !valid {
                    return Err("static assembly value is not a checked literal or local declaration".into());
                }
            }
        }
        Ok(())
    }
}

/// Recognize a closed, inert static singleton declaration independently of
/// whether it is the selected entry receiver. This proves startup can be
/// ignored, not that code can read the object by guessing its declared type.
pub(crate) fn inert_singleton(program: &JavaProgram, owner: &JavaClass, field: &JavaField) -> bool {
    inert_singleton_target(program, owner, field).is_some()
}

fn inert_singleton_target<'a>(program: &'a JavaProgram, owner: &JavaClass, field: &JavaField) -> Option<&'a JavaClass> {
    if !field.is_static() || !field.is_final() || !field.annotations().is_empty()
        || owner.is_interface() || owner.supertype().is_some() || !owner.static_initializers().is_empty()
    {
        return None;
    }
    let JavaType::Class(target) = field.java_type() else { return None; };
    let Some(JavaExpr::New { java_type, args }) = field.initializer() else { return None; };
    if java_type != field.java_type() { return None; }
    let class = program.find_class(target)?;
    if !SourceReceiver::certify(program, owner, class, args).ok()?.is_empty() { return None; }
    Some(class)
}

/// Find a statically assembled receiver invoked inside ordinary Java code.
pub(crate) fn inert_instance_method<'a>(
    program: &'a JavaProgram, caller: &JavaClass, owner: &JavaClass, field: &str, name: &str, arity: usize,
) -> Option<(&'a JavaClass, &'a simulator_parser::JavaMethod, String)> {
    let field = owner.fields().iter().find(|decl| decl.name() == field)?;
    let class = inert_singleton_target(program, owner, field)?;
    // A distinct static receiver may use its certified external bindings.
    // A mutable E snapshot needs a separate rooted identity and is not
    // interchangeable with the selected entry receiver's existing state.
    if class.fields().iter().any(|f| !f.is_static() && !crate::normalized::source_ports::marked(class, f)) {
        return None;
    }
    let mut candidates = class.methods().iter().enumerate().filter(|(_, m)|
        m.name() == name && m.arity() == arity && !m.is_constructor());
    let (index, method) = candidates.next()?;
    (candidates.next().is_none() && !method.is_static() && method.body().is_some()
        && crate::normalized::instance_member_visible(caller, class, method)).then(||
        (class, method, crate::normalized::method_key(class, index, method)))
}

fn assembly_argument<'a>(program: &'a JavaProgram, ty: &JavaType, arg: &JavaExpr) -> Option<AssemblyArgument<'a>> {
    use AssemblyArgument::{Literal, Object};
    match (ty, arg) {
        (JavaType::Primitive(name), JavaExpr::Lit(JavaLit::Bool(_))) if name == "boolean" => Some(Literal),
        (JavaType::Primitive(name), JavaExpr::Lit(JavaLit::Int(_))) if matches!(name.as_str(), "int" | "long") => Some(Literal),
        (JavaType::Primitive(name), JavaExpr::Lit(JavaLit::Long(_))) if name == "long" => Some(Literal),
        (JavaType::Class(name), JavaExpr::Lit(JavaLit::Str(_) | JavaLit::StrUnits(_)))
            if matches!(name.as_str(), "String" | "java.lang.String") => Some(Literal),
        (_, JavaExpr::New { .. }) => allocated_dependency(program, ty, arg).map(Object),
        _ => None,
    }
}

fn allocated_dependency<'a>(program: &'a JavaProgram, declared: &JavaType, arg: &JavaExpr) -> Option<&'a JavaClass> {
    let JavaExpr::New { java_type: actual @ JavaType::Class(name), .. } = arg else { return None; };
    let class = program.find_class(name)?;
    (class.fields().is_empty() && assembly_allocation(program, actual, arg)
        && (declared == actual || crate::normalized::specialized_interface(program, declared).is_some()
            && crate::normalized::implements_specialization(program, class, declared))).then_some(class)
}

pub(crate) fn assembly_allocation(program: &JavaProgram, ty: &JavaType, arg: &JavaExpr) -> bool {
    let (JavaType::Class(name), JavaExpr::New { java_type, args }) = (ty, arg) else { return false; };
    ty == java_type && args.is_empty() && program.find_class(name).is_some_and(|class|
        !class.is_interface() && class.supertype().is_none() && class.static_initializers().is_empty()
        && class.has_trivial_noarg_constructor()
        && class.fields().iter().all(|f| !f.is_static() && !f.has_initializer()))
}

impl SourceEntry {
    pub(super) fn resolve(program: &JavaProgram, root: &Path, locator: &str) -> Result<Self> {
        let diagnostic_location = locator.split_once('#').map_or(locator, |(_, invocation)| invocation);
        let fail = |detail: &str| crate::preprocess::required(
            "NORM-ENTRY", diagnostic_location, &format!("{detail}; source locator: {locator}"),
            "使用能唯一确定源码声明的工程相对路径和静态入口调用。",
        );
        let (file, invocation) = locator.split_once('#').ok_or_else(||
            fail("expected project-relative Java path#static invocation"))?;
        // Insist on the canonical path spelling rather than silently cleaning it.
        if file.split('/').any(|part| part.is_empty() || matches!(part, "." | ".."))
            || file.contains('\\')
            || !Path::new(file).components().all(|part| matches!(part, Component::Normal(_)))
            || Path::new(file).extension().and_then(|ext| ext.to_str()) != Some("java")
        {
            return Err(fail("expected a project-relative .java file"));
        }
        let (invocation, signature) = match invocation.split_once('(') {
            Some((prefix, signature)) => {
                let signature = signature.strip_suffix(')').filter(|sig| !sig.contains(['(', ')']))
                    .ok_or_else(|| fail("malformed source parameter signature"))?;
                let types = if signature.trim().is_empty() { Vec::new() }
                    else { signature.split(',').map(str::trim).collect() };
                if types.iter().any(|ty| ty.is_empty()) {
                    return Err(fail("empty source parameter type"));
                }
                (prefix, Some(types))
            }
            None => (invocation, None),
        };
        let (qualified_owner, method_name) = invocation.rsplit_once('.')
            .ok_or_else(|| fail("expected Class.method or Class.FIELD.method"))?;
        if method_name.starts_with("<lambda:") {
            return Err(fail("local lambda helper is not a source-declared entry"));
        }
        let method_owner = program.find_class(qualified_owner);
        let receiver_root = qualified_owner.rsplit_once('.').and_then(|(owner, name)| {
            let class = program.find_class(owner)?;
            let field = class.fields().iter().find(|field| field.name() == name)?;
            Some((class, field))
        });
        let (class, receiver_field) = match (method_owner, receiver_root) {
            (Some(class), None) => (class, None),
            (None, Some((class, field))) => (class, Some(field)),
            (Some(_), Some(_)) => return Err(fail("ambiguous class or static receiver binding")),
            (None, None) => return Err(fail("undeclared invocation root class or receiver")),
        };
        let root_name = class.full_name();
        let declaration_file = class.source_file();
        if Path::new(declaration_file).strip_prefix(root).ok() != Some(Path::new(file)) {
            return Err(fail("file does not declare the invocation root class"));
        }
        let (target_class, receiver) = if let Some(field) = receiver_field {
            let JavaType::Class(target) = field.java_type() else {
                return Err(fail("receiver field must have a named class type"));
            };
            let args = match field.initializer() {
                Some(JavaExpr::New { java_type, args })
                    if field.is_static() && field.is_final() && java_type == field.java_type() => args,
                _ => return Err(fail("receiver field must be static final with a matching explicit constructor")),
            };
            let declaration = program.find_class(target).ok_or_else(|| fail("missing receiver assembly type"))?;
            for port in declaration.fields().iter().filter(|field| crate::normalized::source_ports::marked(declaration, field)) {
                crate::normalized::source_ports::endpoint_declaration(program, port)?;
            }
            let mut receiver = SourceReceiver {
                owner: root_name.clone(), field: field.name().into(), assembly_fields: BTreeSet::new(),
                links: BTreeMap::new(), startup_setters: BTreeSet::new(),
            };
            let dependencies = SourceReceiver::certify(program, class, declaration, args).map_err(|detail| fail(&detail))?;
            if !dependencies.is_empty() { receiver.links.insert(field.name().into(), dependencies); }
            receiver.certify_static_assembly(program, class).map_err(|detail| fail(&detail))?;
            (declaration, Some(receiver))
        } else {
            (class, None)
        };
        let compatible = target_class.methods().iter().enumerate().filter(|(_, method)|
            !method.is_constructor() && method.name() == method_name
                && method.is_static() == receiver.is_none());
        let overloaded = compatible.clone().nth(1).is_some();
        let mut candidates = compatible.filter(|(_, method)|
            signature.as_ref().is_none_or(|types| method.params().iter()
                .map(|(ty, _)| java_type_spelling(ty)).eq(types.iter().copied().map(str::to_owned))));
        let (index, method) = candidates.next().ok_or_else(|| fail("no entry method matches the invocation and source parameter signature"))?;
        if candidates.next().is_some() {
            return Err(crate::preprocess::required(
                "NORM-OVERLOAD", invocation, "ambiguous entry method", "入口需要显式的 Java 源码参数类型签名。",
            ));
        }
        if receiver.is_some() && !crate::normalized::instance_member_visible(class, target_class, method) {
            let reason = if target_class.package() == class.package() {
                "private receiver entry is not accessible from the assembly class"
            } else { "cross-package receiver entry must name a public method" };
            return Err(fail(reason));
        }

        // The caller may include a redundant signature. PERS identifies
        // the bound declaration, not the spelling used to request it.
        let canonical_root = receiver.as_ref().map_or(root_name.clone(), |receiver|
            format!("{root_name}.{}", receiver.field));
        let signature = if overloaded {
            format!("({})", method.params().iter().map(|(ty, _)| java_type_spelling(ty))
                .collect::<Vec<_>>().join(", "))
        } else { String::new() };
        Ok(Self {
            locator: locator.into(), canonical_function: format!("{file}#{canonical_root}.{method_name}{signature}"),
            project_root: root.to_path_buf(), target: crate::normalized::method_key(target_class, index, method), receiver,
        })
    }
}


fn java_type_spelling(ty: &JavaType) -> String {
    match ty {
        JavaType::Primitive(name) | JavaType::Class(name) => name.clone(),
        JavaType::Array(element) => format!("{}[]", java_type_spelling(element)),
        JavaType::Generic(name, args) => format!("{name}<{}>",
            args.iter().map(java_type_spelling).collect::<Vec<_>>().join(", ")),
        JavaType::Void => "void".into(),
    }
}
