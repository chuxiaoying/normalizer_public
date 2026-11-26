//! Source recognition for closed normalized text intrinsics.
pub use math::domains::text::{is_blank, trim, whitespace, KEY};
pub(crate) use math::domains::text::TRIM_KEY;

pub(crate) const SOURCE_IMPORT: &str = "normalizer.intrinsics.NormalizedText";
pub(crate) enum SourceIntrinsic {
    Trim,
    DefaultIfBlank,
}

pub(crate) fn source_intrinsic(
    class: &simulator_parser::JavaClass,
    target: &str,
    arity: usize,
) -> Option<SourceIntrinsic> {
    if !class.imports().iter().any(|import| import == SOURCE_IMPORT) {
        return None;
    }
    match (target, arity) {
        ("NormalizedText.trim", 1) => Some(SourceIntrinsic::Trim),
        ("NormalizedText.defaultIfBlank", 2) => Some(SourceIntrinsic::DefaultIfBlank),
        _ => None,
    }
}
