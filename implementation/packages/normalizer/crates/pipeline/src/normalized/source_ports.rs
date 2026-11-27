//! A source port is a proven injected field marked @N.External and a declared
//! external method. Bodylessness alone, including native, never makes a port.
use super::*;
use simulator_parser::{JavaClass, JavaField};

pub(crate) const IMPORT: &str = "normalizer.intrinsics.N.External";

pub(crate) fn marked(class: &JavaClass, field: &JavaField) -> bool {
    official_marker(class, field.annotations(), IMPORT)
}

pub(crate) fn receiver(expr: &E) -> Option<&str> {
    match expr {
        E::Ident(name) => Some(name),
        E::FieldAccess { target, field } if matches!(target.as_ref(),E::Ident(name) if name=="this") => {
            Some(field)
        }
        _ => None,
    }
}

pub(crate) fn declares_bodyless_member(class: &JavaClass, member: &str, arity: usize) -> bool {
    let mut declarations = class.methods().iter()
        .filter(|method| method.name() == member && method.arity() == arity);
    declarations.next().is_some_and(|method| external_member(class, method))
        && declarations.next().is_none()
}

fn external_member(class: &JavaClass, method: &simulator_parser::JavaMethod) -> bool {
    method.body().is_none()
        && !method.is_static()
        && !method.is_constructor()
        && (class.is_interface() || method.is_native())
}

#[derive(Clone)]
pub(crate) struct SourcePort {
    pub interface: String,
    pub source_path: String,
    pub endpoint: String,
    pub returns: JavaType,
    pub params: Vec<(JavaType, String)>,
    pub throws: Vec<String>,
}

/// Resolve the actual source-declared port class before analyzing a
/// receiver's assembly. A malformed declaration must not be diagnosed as
/// merely an incompatible constructor argument.
pub(crate) fn endpoint_declaration<'a>(program: &'a JavaProgram, field: &JavaField) -> Result<&'a JavaClass, String> {
    if field.is_static() || field.has_initializer() {
        return Err(error("NORM-EXTERNAL-FIELD", field.name()));
    }
    let JavaType::Class(endpoint) = field.java_type() else {
        return Err(error("NORM-EXTERNAL-TYPE", field.name()));
    };
    program.find_class(endpoint)
        .filter(|class| class.is_interface() || class.methods().iter().any(|method| external_member(class, method)))
        .ok_or_else(|| error("NORM-EXTERNAL-TYPE", endpoint))
}

pub(crate) fn declarations(
    program: &JavaProgram,
    exceptions: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, SourcePort>, String> {
    let mut ports = BTreeMap::new();
    for owner in program.classes() {
        for field in owner.fields().iter().filter(|field| marked(owner, field)) {
            let declaration = endpoint_declaration(program, field)?;
            let interface = declaration.full_name();
            for method in declaration.methods() {
                // The constructor, if present, is assembly, not a port member.
                if !declaration.is_interface() && method.is_constructor() { continue; }
                if !external_member(declaration, method)
                    || !method.annotations().is_empty()

                    || method
                        .params()
                        .iter()
                        .any(|(_, arg)| !method.param_annotations(arg).is_empty())
                {
                    return Err(error("NORM-EXTERNAL-METHOD", method.name()));
                }
                let throws = method
                    .throws()
                    .iter()
                    .map(|name| {
                        exceptions
                            .contains_key(name)
                            .then(|| name.clone())
                            .ok_or_else(|| error("NORM-EXTERNAL-EXCEPTION", name))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let endpoint = format!("{}.{}", field.name(), method.name());
                let id = format!("{}.{endpoint}", owner.full_name());
                if ports
                    .insert(
                        id.clone(),
                        SourcePort {
                            interface: interface.clone(),
                            source_path: declaration.source_file().unwrap_or(&interface).to_owned(),
                            endpoint,
                            returns: method.return_type().clone(),
                            params: method.params().to_vec(),
                            throws,
                        },
                    )
                    .is_some()
                {
                    return Err(error("NORM-OVERLOAD", &id));
                }
            }
        }
    }
    Ok(ports)
}
