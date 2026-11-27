//! Pure Java-compatible text content operations. Call constructors are typed here.
use crate::{builder as b, Expr, Lit, Primitive};
use crate::domains::primitive::registry::FuncSemantics;

pub const TRIM_KEY: &str = "normalized.text.trim_utf16";
pub const DEFAULT_IF_BLANK_KEY: &str = "normalized.text.default_if_blank";

static DEFAULT_IF_BLANK: std::sync::OnceLock<crate::FuncKey> = std::sync::OnceLock::new();

/// AI-02 / BF-05: a single operation over values already evaluated by the host.
pub fn default_if_blank(value: Expr, fallback: Expr) -> Expr {
    let key = *DEFAULT_IF_BLANK.get_or_init(|| crate::register_func(
        DEFAULT_IF_BLANK_KEY,
        vec![Primitive::String, Primitive::String],
        Some(Primitive::String),
        FuncSemantics::pure_function(),
    ));
    crate::normalize(b::call(key, vec![value, fallback]))
}

pub(crate) fn fold_default_if_blank(key: crate::FuncKey, args: &[Expr]) -> Option<Expr> {
    if DEFAULT_IF_BLANK.get() != Some(&key) { return None; }
    let [value, fallback] = args else { return None; };
    if value == fallback { return Some(value.clone()); }
    match value.as_lit()? {
        Lit::Null => Some(fallback.clone()),
        Lit::Str(text) => Some(if text.encode_utf16().all(whitespace) { fallback } else { value }.clone()),
        _ => None,
    }
}

/// When null was explicitly handled with the same fallback, both branches
/// already have the same value and fallback available. Use the registered
/// null contract rather than a second spelling of this operation.
pub(crate) fn redundant_null_default(expr: &Expr) -> Option<Expr> {
    use crate::{BinOpKind, UnaryOpKind};
    let ([(condition, then_value)], else_value) = expr.as_total_case_when()? else { return None; };
    let (guard, then_value, else_value) =
        if let Some((UnaryOpKind::Not, guard)) = condition.as_unary_op() {
            (guard, else_value, then_value)
        } else { (condition, then_value, else_value) };
    let (op, left, right) = guard.as_binop()?;
    let (on_null, otherwise) = match op {
        BinOpKind::Eq => (then_value, else_value),
        BinOpKind::Ne => (else_value, then_value),
        _ => return None,
    };
    let subject = if left.is_null() { right } else if right.is_null() { left } else { return None; };
    let (key, [value, fallback]) = otherwise.as_call()? else { return None; };
    (DEFAULT_IF_BLANK.get() == Some(&key) && value == subject && fallback == on_null)
        .then(|| otherwise.clone())
}

static TRIM: std::sync::OnceLock<crate::FuncKey> = std::sync::OnceLock::new();

pub fn trim(value: Expr) -> Expr {
    let key = *TRIM.get_or_init(|| crate::register_func(
        TRIM_KEY,
        vec![Primitive::String],
        Some(Primitive::String),
        FuncSemantics::pure_function()
            .with_null_behavior(vec![0], Lit::Null)
            .with_evaluator(std::sync::Arc::new(|args| evaluate_text(args, |text| {
                Lit::Str(text.trim_matches(|c: char| c <= '\u{20}').into())
            }))),
    ));
    crate::normalize(b::call(key, vec![value]))
}

/// A nested use of the same typed trim is already a complete trim.
pub(crate) fn repeat_trim(key: crate::FuncKey, args: &[Expr]) -> Option<Expr> {
    if TRIM.get() != Some(&key) { return None; }
    let value = args.first()?;
    value.as_call().is_some_and(|(inner, _)| inner == key).then(|| value.clone())
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
