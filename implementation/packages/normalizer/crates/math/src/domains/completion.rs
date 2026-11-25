//! Closed disjoint completion values: Return(value) | Throw(class, payload).
//!
//! A boundary's declared variants are exhaustive and mutually exclusive.
//! is_throw is total. value/payload are partial projections, defined only in
//! the corresponding variant; callers must retain the appropriate domain.
use crate::{builder as b, Expr, FuncKey, FuncSemantics, Primitive};

pub fn output_name(name: &str) -> &str {
    match name.split_once('@') {
        Some((base, "number" | "boolean" | "string" | "identity" | "composite"))
            if matches!(
                base,
                "completion.return"
                    | "completion.throw"
                    | "completion.value"
                    | "completion.payload"
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
pub fn returned(value: Expr, ty: Primitive) -> Expr {
    typed(
        "completion.return",
        vec![value],
        vec![ty],
        Primitive::Composite,
        ty,
    )
}
pub fn thrown(class: &str, payload: Expr, ty: Primitive) -> Expr {
    typed(
        "completion.throw",
        vec![b::lit_str(class), payload],
        vec![Primitive::String, ty],
        Primitive::Composite,
        ty,
    )
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
pub fn reduce(key: FuncKey, args: &[Expr]) -> Option<Expr> {
    let name = crate::func_name(key);
    if !known(&name) {
        return None;
    }
    let (subject, requested) = match args {
        [value] => (value, None),
        [value, class] => (value, Some(class)),
        _ => return None,
    };
    let (constructor, values) = subject.as_call()?;
    let constructor = crate::func_name(constructor);
    if !known(&constructor) {
        return None;
    }
    let kind = output_name(&constructor);
    let same_type =
        name.split_once('@').map(|(_, ty)| ty) == constructor.split_once('@').map(|(_, ty)| ty);
    match (output_name(&name), kind, values) {
        ("completion.is_throw", "completion.return", [_]) => Some(b::lit_bool(false)),
        ("completion.is_throw", "completion.throw", [class, _]) => {
            Some(crate::normalize(b::eq(class.clone(), requested?.clone())))
        }
        ("completion.value", "completion.return", [value]) if same_type => Some(value.clone()),
        ("completion.payload", "completion.throw", [class, value])
            if same_type && Some(class) == requested =>
        {
            Some(value.clone())
        }
        _ => None,
    }
}
