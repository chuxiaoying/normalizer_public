//! Symbolic observations of the runtime tag of an untrusted source value.
//! The typed projection is not a conversion. The caller must introduce it
//! only on the success branch of the corresponding native type guard.
use crate::{builder as b, Expr, Primitive};
use crate::domains::primitive::registry::FuncSemantics;

pub const TYPEOF: &str = "normalized.ts.typeof";
pub const TEXT_PROJECTION: &str = "normalized.ts.narrow_text";

pub fn is_native_text(value: Expr) -> Expr {
    let key = crate::register_func(TYPEOF, vec![Primitive::Composite],
        Some(Primitive::String), FuncSemantics::pure_function());
    b::eq(b::call(key, vec![value]), b::lit_str("string"))
}

pub fn certified_text(value: Expr) -> Expr {
    let key = crate::register_func(TEXT_PROJECTION, vec![Primitive::Composite],
        Some(Primitive::String), FuncSemantics::pure_function());
    b::call(key, vec![value])
}
