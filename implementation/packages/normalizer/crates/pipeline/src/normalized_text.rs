//! Source recognition for closed normalized text intrinsics.
pub use math::domains::text::{is_blank, trim, whitespace, KEY};
pub(crate) use math::domains::text::TRIM_KEY;

pub(crate) const SOURCE_IMPORT: &str = "normalizer.intrinsics.NormalizedText";
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
    let bound = match receiver {
        "N" => crate::normalized::imports_official_n(class),
        "NormalizedText" => class.imports().iter().any(|import| import == SOURCE_IMPORT),
        _ => false,
    };
    bound && !class.fields().iter().any(|field| field.name() == receiver)
        && !program.classes().iter().any(|ty| ty.simple_name().rsplit('.').next() == Some(receiver))
}
