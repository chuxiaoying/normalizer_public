//! Resolve public source entry identity before the execution key is selected.
use super::Result;
use simulator_parser::{JavaClass, JavaExpr, JavaField, JavaLit, JavaProgram, JavaStatement, JavaType};
use std::collections::BTreeSet;
use std::path::{Component, Path};

#[derive(Debug)]
pub(super) struct SourceEntry {
    pub(super) locator: String,
    pub(super) target: String,
    pub(super) receiver: Option<SourceReceiver>,
}

#[derive(Debug)]
pub(super) struct SourceReceiver {
    pub(super) owner: String,
    pub(super) field: String,
    pub(super) assembly_fields: BTreeSet<String>,
}

/// The implicit constructor of an ordinary class has no parameters. A
/// record instead has its canonical component parameters, even though the
/// source parser does not synthesize a constructor declaration.
fn has_zero_argument_implicit_constructor(class: &JavaClass) -> bool {
    !class.is_record() || class.fields().is_empty()
}

impl SourceReceiver {
    /// Establish the identity of a pre-existing receiver, not a fresh Java
    /// allocation in the analyzed invocation. Only straight-line, non-throwing
    /// assembly is admitted by this route.
    fn certify(program: &JavaProgram, target: &str, initializer: &JavaExpr) -> Result<()> {
        let JavaExpr::New { args, .. } = initializer else { unreachable!() };
        let class = program.find_class(target).ok_or("missing receiver assembly type")?;
        let mut constructors = class.methods().iter().filter(|m| m.is_constructor());
        let constructor = constructors.next();
        if constructors.next().is_some() || class.is_interface() || class.supertype().is_some() || !class.static_initializers().is_empty() {
            return Err("unsupported receiver assembly class".into());
        }
        if constructor.is_none() && !has_zero_argument_implicit_constructor(class) {
            return Err("record components require a canonical constructor; not inert receiver assembly".into());
        }
        let params = constructor.map(|m| m.params()).unwrap_or_default();
        if params.len() != args.len() || !params.iter().zip(args).all(|((ty, _), arg)| assembly_argument(program, ty, arg)) {
            return Err("unsupported or mismatched receiver constructor arguments".into());
        }
        let mut attached_ports = BTreeSet::new();
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
                    if !matches!(args[index], JavaExpr::New { .. }) {
                        return Err("external receiver field must be bound to a declared port object".into());
                    }
                    attached_ports.insert(field.as_str());
                } else if !crate::normalized::entry_state(class, declaration) {
                    return Err("receiver constructor writes a field outside its entry contract".into());
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
        Ok(())
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
                if receiver == &self.field && !crate::normalized::entry_state(declaration, member) {
                    return Err("static assembly may only write declared state on the entry receiver".into());
                }
                let assigned_type = member.java_type();
                let valid = match value {
                    JavaExpr::Ident(name) => initialized.get(name.as_str()).copied() == Some(assigned_type),
                    JavaExpr::ArrayAllocation { element_type, length }
                        if matches!(assigned_type, JavaType::Array(item) if item.as_ref() == element_type)
                            && matches!(element_type, JavaType::Primitive(name) if name == "int") =>
                        matches!(length.as_ref(), JavaExpr::Lit(JavaLit::Int(n)) if (0..=256).contains(n)),
                    _ => assembly_argument(program, assigned_type, value) || assembly_allocation(program, assigned_type, value),
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
    if !field.is_static() || !field.is_final() || !field.annotations().is_empty()
        || owner.is_interface() || owner.supertype().is_some() || !owner.static_initializers().is_empty()
    {
        return false;
    }
    let JavaType::Class(target) = field.java_type() else { return false; };
    let Some(initializer @ JavaExpr::New { java_type, .. }) = field.initializer() else { return false; };
    java_type == field.java_type() && SourceReceiver::certify(program, target, initializer).is_ok()
}

fn assembly_argument(program: &JavaProgram, ty: &JavaType, arg: &JavaExpr) -> bool {
    match (ty, arg) {
        (JavaType::Primitive(name), JavaExpr::Lit(JavaLit::Bool(_))) => name == "boolean",
        (JavaType::Primitive(name), JavaExpr::Lit(JavaLit::Int(_))) => matches!(name.as_str(), "int" | "long"),
        (JavaType::Primitive(name), JavaExpr::Lit(JavaLit::Long(_))) => name == "long",
        (JavaType::Class(name), JavaExpr::Lit(JavaLit::Str(_))) => matches!(name.as_str(), "String" | "java.lang.String"),
        (JavaType::Class(name), JavaExpr::New { java_type, args }) if ty == java_type && args.is_empty() =>
            program.find_class(name).is_some_and(|class| class.fields().is_empty())
                && assembly_allocation(program, ty, arg),
        _ => false,
    }
}

pub(crate) fn assembly_allocation(program: &JavaProgram, ty: &JavaType, arg: &JavaExpr) -> bool {
    let (JavaType::Class(name), JavaExpr::New { java_type, args }) = (ty, arg) else { return false; };
    ty == java_type && args.is_empty() && program.find_class(name).is_some_and(|class|
        !class.is_interface() && class.supertype().is_none() && class.static_initializers().is_empty()
        && has_zero_argument_implicit_constructor(class)
        && class.fields().iter().all(|f| !f.is_static() && !f.has_initializer())
        && !class.methods().iter().any(|method| method.is_constructor()))
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
        let method_owner = program.find_class(qualified_owner);
        let receiver_root = qualified_owner.rsplit_once('.').and_then(|(owner, name)| {
            let class = program.find_class(owner)?;
            class.fields().iter().any(|field| field.name() == name).then_some((class, name))
        });
        let (class, receiver_name) = match (method_owner, receiver_root) {
            (Some(class), None) => (class, None),
            (None, Some((class, name))) => (class, Some(name)),
            (Some(_), Some(_)) => return Err(fail("ambiguous class or static receiver binding")),
            (None, None) => return Err(fail("undeclared invocation root class or receiver")),
        };
        let root_name = class.full_name();
        let declaration_file = class.source_file().ok_or_else(|| fail("missing invocation root source declaration"))?;
        if Path::new(declaration_file).strip_prefix(root).ok() != Some(Path::new(file)) {
            return Err(fail("file does not declare the invocation root class"));
        }
        let (target, receiver) = if let Some(name) = receiver_name {
            let field = class.fields().iter().find(|field| field.name() == name)
                .ok_or_else(|| fail("undeclared receiver field"))?;
            let JavaType::Class(target) = field.java_type() else {
                return Err(fail("receiver field must have a named class type"));
            };
            if !field.is_static() || !field.is_final()
                || !matches!(field.initializer(), Some(JavaExpr::New { java_type, .. }) if java_type == field.java_type())
            {
                return Err(fail("receiver field must be static final with a matching explicit constructor"));
            }
            let mut receiver = SourceReceiver { owner: root_name.clone(), field: name.into(), assembly_fields: BTreeSet::new() };
            if let Some(declaration) = program.find_class(target) {
                for port in declaration.fields().iter().filter(|field| crate::normalized::source_ports::marked(declaration, field)) {
                    crate::normalized::source_ports::endpoint_declaration(program, port)?;
                }
            }
            SourceReceiver::certify(program, target, field.initializer().unwrap()).map_err(|detail| fail(&detail))?;
            receiver.certify_static_assembly(program, class).map_err(|detail| fail(&detail))?;
            (target.as_str(), Some(receiver))
        } else {
            (root_name.as_str(), None)
        };
        let target_class = program.find_class(target).ok_or_else(|| fail("undeclared receiver class"))?;
        let mut candidates = target_class.methods().iter().enumerate().filter(|(_, method)|
            !method.is_constructor() && method.name() == method_name
                && signature.as_ref().is_none_or(|types| method.params().iter()
                    .map(|(ty, _)| java_type_spelling(ty)).eq(types.iter().copied().map(str::to_owned))));
        let (index, method) = candidates.next().ok_or_else(|| fail("undeclared entry method or source parameter signature"))?;
        if candidates.next().is_some() {
            return Err(crate::preprocess::required(
                "NORM-OVERLOAD", invocation, "ambiguous entry method", "入口需要显式的 Java 源码参数类型签名。",
            ));
        }
        if method.is_static() != receiver.is_none() {
            return Err(fail("invocation does not match the entry method's static/instance declaration"));
        }
        Ok(Self { locator: locator.into(), target: crate::normalized::method_key(target_class, index, method), receiver })
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

#[cfg(test)]
mod assembly_constructor_contract {
    use super::*;
    use simulator_parser::{parse, ParseOptions};

    #[test]
    fn a_nonempty_record_is_not_a_default_zero_argument_producer() {
        for (declaration, admissible) in [
            ("class Payload { int value; }", true),
            ("record Payload() {}", true),
            ("record Payload(int value) {}", false),
        ] {
            let source = format!("{declaration} class Entry {{ static final Payload ROOT = new Payload(); }}");
            let program = parse(&source, "Entry.java", &ParseOptions::default()).unwrap();
            let owner = program.find_class("Entry").unwrap();
            let field = &owner.fields()[0];
            let initializer = field.initializer().unwrap();
            assert_eq!(assembly_allocation(&program, field.java_type(), initializer), admissible, "{declaration}");
            assert_eq!(inert_singleton(&program, owner, field), admissible, "{declaration}");
        }

        let source = "import normalizer.intrinsics.N;
          record Payload(int value) {}
          class Entry {
            @N.EntryState Payload seed = new Payload();
            int read() { return 0; }
          }";
        let program = parse(source, "Entry.java", &ParseOptions::default()).unwrap();
        let result = crate::normalized::source_layouts(&program);
        let error = result.expect_err("an invalid canonical constructor cannot produce acquired state");
        assert!(error.contains("[NORM-FIELD-DECLARATION]"), "{error}");
    }
}
