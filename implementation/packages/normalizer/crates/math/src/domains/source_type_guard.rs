//! Symbolic observations of the runtime tag of an untrusted source value.
//! The typed projection is not a conversion. The caller must introduce it
//! only on the success branch of the corresponding native type guard.
use crate::{builder as b, BinOpKind, Expr, Primitive};
use crate::domains::primitive::registry::FuncSemantics;

pub const TYPEOF: &str = "normalized.source.typeof";
pub const TEXT_PROJECTION: &str = "normalized.source.narrow_text";

pub fn is_native_text(value: Expr) -> Expr {
    let key = crate::register_func(TYPEOF, vec![Primitive::Composite],
        Some(Primitive::String), FuncSemantics::pure_function());
    b::eq(b::call(key, vec![value]), b::lit_str("string"))
}

pub fn is_positive_text_guard(expr: &Expr) -> bool {
    let Some((BinOpKind::Eq, left, right)) = expr.as_binop() else { return false; };
    let tag = |expr: &Expr| expr.as_call().is_some_and(|(key, args)| {
        args.len() == 1 && crate::func_name(key) == TYPEOF
    });
    let text = |expr: &Expr| expr == &b::lit_str("string");
    (tag(left) && text(right)) || (tag(right) && text(left))
}

pub fn is_native_unknown_parameter(expr: &Expr) -> bool {
    matches!(expr.as_sym_var(),
        Some(crate::SymVarOrigin::Parameter { primitive_type, .. })
            if primitive_type == crate::ScalarSymbolType::UntrustedSourceValue.tag())
}

pub fn certified_text(value: Expr) -> Expr {
    let key = crate::register_func(TEXT_PROJECTION, vec![Primitive::Composite],
        Some(Primitive::String), FuncSemantics::pure_function());
    b::call(key, vec![value])
}
