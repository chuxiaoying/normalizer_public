//! Closed disjoint completion values: Return(value) | Throw(class, payload).
//!
//! A boundary's declared variants are exhaustive and mutually exclusive.
//! is_throw is total. value/payload are partial projections, defined only in
//! the corresponding variant; callers must retain the appropriate domain.
use crate::{builder as b, Expr, FuncSemantics, Primitive};

pub fn output_name(name: &str) -> &str {
    match name.split_once('@') {
        Some((base, "number" | "boolean" | "string" | "identity" | "composite"))
            if matches!(
                base,
                "completion.value" | "completion.payload"
            ) =>
        {
            base
        }
        _ => name,
    }
}
pub fn known(name: &str) -> bool {
    name == "completion.is_throw" || (name.contains('@') && output_name(name) != name)
}
fn typed(
    name: &str,
    args: Vec<Expr>,
    params: Vec<Primitive>,
    result: Primitive,
    payload_type: Primitive,
) -> Expr {
    let key = crate::register_func(
        &format!("{name}@{}", payload_type.algebra_name()),
        params,
        Some(result),
        FuncSemantics::pure_function(),
    );
    crate::normalize(b::call(key, args))
}
pub fn is_throw(value: Expr, class: &str) -> Expr {
    let key = crate::register_func(
        "completion.is_throw",
        vec![Primitive::Composite, Primitive::String],
        Some(Primitive::Boolean),
        FuncSemantics::pure_function(),
    );
    crate::normalize(b::call(key, vec![value, b::lit_str(class)]))
}
pub fn value(completion: Expr, ty: Primitive) -> Expr {
    typed(
        "completion.value",
        vec![completion],
        vec![Primitive::Composite],
        ty,
        ty,
    )
}
pub fn payload(completion: Expr, class: &str, ty: Primitive) -> Expr {
    typed(
        "completion.payload",
        vec![completion, b::lit_str(class)],
        vec![Primitive::Composite, Primitive::String],
        ty,
        ty,
    )
}
