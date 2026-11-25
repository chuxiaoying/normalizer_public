//! Closed text operation. Nullable content, not Java String object identity.
use math::{builder as b, Expr, Lit, Primitive};
pub const KEY: &str = "normalized.text.is_blank_java17";
pub(crate) const SOURCE_IMPORT: &str = "normalizer.intrinsics.NormalizedText";
pub(crate) const TRIM_KEY: &str = "normalized.text.trim_utf16";
pub(crate) fn source_intrinsic(class: &simulator_parser::JavaClass, target: &str, arity: usize) -> bool {
    class.imports().iter().any(|import| import == SOURCE_IMPORT)
        && matches!((target,arity), ("NormalizedText.trim",1) | ("NormalizedText.defaultIfBlank",2))
}

pub fn trim(value: Expr) -> Expr {
    let key = math::register_func(
        TRIM_KEY,
        vec![Primitive::String],
        Some(Primitive::String),
        math::domains::primitive::registry::FuncSemantics::pure_function()
            .with_null_behavior(vec![0], Lit::Null)
            .with_evaluator(std::sync::Arc::new(|args| match args {
                [Lit::Str(s)] => Some(Lit::Str(s.trim_matches(|c: char| c <= '\u{20}').into())),
                _ => None,
            }))
            .with_symbolic_reducer(std::sync::Arc::new(|trim, args| {
                let [value] = args else { return None; };
                let (inner, _) = value.as_call()?;
                (inner == trim).then(|| value.clone())
            })),
    );
    math::normalize(b::call(key, vec![value]))
}

/// Character.isWhitespace(char), Java 17: Unicode spaces excluding NBSPs,
/// together with Java's ASCII whitespace/control ranges. Surrogates are not spaces.
pub fn whitespace(unit: u16) -> bool {
    matches!(unit, 0x0009..=0x000d | 0x001c..=0x0020 | 0x1680 |
        0x2000..=0x2006 | 0x2008..=0x200a | 0x2028..=0x2029 | 0x205f | 0x3000)
}
fn evaluate_non_null(args: &[Lit]) -> Option<Lit> {
    match args {
        [Lit::Str(s)] => Some(Lit::Bool(s.encode_utf16().all(whitespace))),
        _ => None,
    }
}
pub fn is_blank(value: Expr) -> Expr {
    let key = math::register_func(
        KEY,
        vec![Primitive::String],
        Some(Primitive::Boolean),
        math::domains::primitive::registry::FuncSemantics::pure_function()
            .with_null_behavior(vec![0], Lit::Bool(true))
            .with_evaluator(std::sync::Arc::new(evaluate_non_null)),
    );
    math::normalize(b::call(key, vec![value]))
}
