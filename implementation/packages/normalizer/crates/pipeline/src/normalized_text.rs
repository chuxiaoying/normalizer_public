//! Source recognition for closed normalized text intrinsics.
pub use math::domains::text::trim;
pub(crate) use math::domains::text::{default_if_blank, DEFAULT_IF_BLANK_KEY, TRIM_KEY};

pub(crate) fn clock_intrinsic(
    program: &simulator_parser::JavaProgram,
    class: &simulator_parser::JavaClass,
    target: &str,
    arity: usize,
) -> bool {
    target == "N.currentTimeMillis" && arity == 0 && bound_receiver(program, class, "N")
}

pub(crate) enum SourceIntrinsic {
    Trim,
    DefaultIfBlank,
}

pub(crate) fn source_intrinsic(
    program: &simulator_parser::JavaProgram,
    class: &simulator_parser::JavaClass,
    target: &str,
    arity: usize,
) -> Option<SourceIntrinsic> {
    let (receiver, member) = target.split_once('.')?;
    if !bound_receiver(program, class, receiver) {
        return None;
    }
    match (member, arity) {
        ("trim", 1) => Some(SourceIntrinsic::Trim),
        ("defaultIfBlank", 2) => Some(SourceIntrinsic::DefaultIfBlank),
        _ => None,
    }
}

pub(crate) fn bound_receiver(
    program: &simulator_parser::JavaProgram,
    class: &simulator_parser::JavaClass,
    receiver: &str,
) -> bool {
    receiver == "N"
        && class.imports_normalizer_intrinsics()
        && !class.fields().iter().any(|field| field.name() == receiver)
        && !namespace_declared_in_unit(program, class)
}

// A declaration in another Java compilation unit cannot impersonate
// the explicit single-type import in this unit.
pub(crate) fn namespace_declared_in_unit(
    program: &simulator_parser::JavaProgram,
    class: &simulator_parser::JavaClass,
) -> bool {
    program.classes().iter().any(|other| {
        other.source_file() == class.source_file()
            && other.simple_name().rsplit('.').next() == Some("N")
    })
}
