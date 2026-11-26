//! Pure Java-compatible text content operations. Call constructors are typed here.
use crate::{builder as b, Expr, Lit, Primitive};
use crate::domains::primitive::registry::FuncSemantics;

pub const KEY: &str = "normalized.text.is_blank_java17";
pub const TRIM_KEY: &str = "normalized.text.trim_utf16";

pub fn trim(value: Expr) -> Expr {
    let key = crate::register_func(
        TRIM_KEY,
        vec![Primitive::String],
        Some(Primitive::String),
        FuncSemantics::pure_function()
            .with_null_behavior(vec![0], Lit::Null)
            .with_evaluator(std::sync::Arc::new(|args| evaluate_text(args, |text| {
                Lit::Str(text.trim_matches(|c: char| c <= '\u{20}').into())
            })))
            .with_unary_idempotence(),
    );
    crate::normalize(b::call(key, vec![value]))
}

/// Character.isWhitespace(char), Java 17: Unicode spaces excluding NBSPs,
/// together with Java's ASCII whitespace/control ranges. Surrogates are not spaces.
pub fn whitespace(unit: u16) -> bool {
    matches!(unit, 0x0009..=0x000d | 0x001c..=0x0020 | 0x1680 |
        0x2000..=0x2006 | 0x2008..=0x200a | 0x2028..=0x2029 | 0x205f | 0x3000)
}
fn evaluate_text(args: &[Lit], evaluate: impl FnOnce(&str) -> Lit) -> Option<Lit> {
    match args {
        [Lit::Str(text)] => Some(evaluate(text)),
        _ => None,
    }
}
pub fn is_blank(value: Expr) -> Expr {
    let key = crate::register_func(
        KEY,
        vec![Primitive::String],
        Some(Primitive::Boolean),
        FuncSemantics::pure_function()
            .with_null_behavior(vec![0], Lit::Bool(true))
            .with_evaluator(std::sync::Arc::new(|args| evaluate_text(args, |text| {
                Lit::Bool(text.encode_utf16().all(whitespace))
            }))),
    );
    crate::normalize(b::call(key, vec![value]))
}
