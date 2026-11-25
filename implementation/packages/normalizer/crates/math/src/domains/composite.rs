//! Opaque immutable inputs and their typed projections.
//! The caller must supply a checked structural type.
use super::primitive::registry::FuncSemantics;
use crate::{builder as b, Expr, Primitive};

pub fn output_name(name: &str) -> &str {
    match name {
        "value.field@number"
        | "value.field@boolean"
        | "value.field@string"
        | "value.field@composite"
        | "value.field@identity" => "value.field",
        _ => name,
    }
}
fn field_intrinsics(
) -> &'static std::sync::RwLock<std::collections::HashMap<crate::FuncKey, Primitive>> {
    static KEYS: std::sync::OnceLock<
        std::sync::RwLock<std::collections::HashMap<crate::FuncKey, Primitive>>,
    > = std::sync::OnceLock::new();
    KEYS.get_or_init(Default::default)
}
/// Evidence of a complete immutable field intrinsic actually registered by
/// this factory. A matching function name or pure return type is insufficient.
pub fn field_intrinsic_result(key: crate::FuncKey) -> Option<Primitive> {
    field_intrinsics().read().unwrap().get(&key).copied()
}

pub fn apply(name: &str, args: Vec<Expr>, params: Vec<Primitive>, result: Primitive) -> Expr {
    assert!(matches!(name, "value.input" | "value.field"), "undefined composite operation");
    let registered = if name == "value.field" {
        format!("{name}@{}", result.algebra_name())
    } else {
        name.into()
    };
    let certified_field =
        name == "value.field" && params == vec![Primitive::Composite, Primitive::String];
    let key = crate::register_func(
        &registered,
        params,
        Some(result),
        FuncSemantics::pure_function(),
    );
    if certified_field {
        field_intrinsics().write().unwrap().insert(key, result);
    }
    crate::normalize(b::call(key, args))
}
// Provenance is kept at the intrinsic, separate from opaque function names.
static VECTOR_LENGTH: std::sync::OnceLock<crate::FuncKey> = std::sync::OnceLock::new();

pub fn certified_vector_length(input: Expr) -> Expr {
    let key = *VECTOR_LENGTH.get_or_init(|| crate::register_func(
        "value.vector_length", vec![Primitive::Composite], Some(Primitive::Number),
        FuncSemantics::pure_function(),
    ));
    b::call(key, vec![input])
}

pub fn vector_length_input(expr: &Expr) -> Option<&Expr> {
    let (key, args) = expr.as_call()?;
    let [input] = args else { return None; };
    (VECTOR_LENGTH.get() == Some(&key)).then_some(input)
}
pub fn input_name(expr: &Expr) -> Option<&str> {
    let (key, args) = expr.as_call()?;
    let [name] = args else { return None; };
    if crate::func_name(key) != "value.input" { return None; }
    match name.as_lit()? { crate::Lit::Str(name) => Some(name), _ => None }
}

pub(crate) fn fold_vector_length_predicate(expr: &Expr) -> Option<Expr> {
    use crate::BinOpKind::*;
    let (op, left, right) = expr.as_binop()?;
    let (op, subject, point) = if vector_length_input(left).is_some() {
        (op, left, right)
    } else if vector_length_input(right).is_some() {
        (match op { Lt => Gt, Le => Ge, Gt => Lt, Ge => Le, _ => op }, right, left)
    } else {
        return None;
    };
    let n = point.as_lit()?.as_integer()?;
    let zero: crate::Integer = 0.into();
    let one: crate::Integer = 1.into();
    let empty = || b::eq(subject.clone(), b::lit_int(0));
    let nonempty = || b::ne(subject.clone(), b::lit_int(0));
    Some(match op {
        Gt if n < zero => b::lit_bool(true),
        Ge if n <= zero => b::lit_bool(true),
        Lt if n <= zero => b::lit_bool(false),
        Le | Eq if n < zero => b::lit_bool(false),
        Ne if n < zero => b::lit_bool(true),
        Gt if n == zero => nonempty(),
        Ge if n == one => nonempty(),
        Lt if n == one => empty(),
        Le if n == zero => empty(),
        Gt => b::ge(subject.clone(), b::lit_integer(n + 1)),
        Lt => b::le(subject.clone(), b::lit_integer(n - 1)),
        _ => return None,
    })
}

pub fn reduce(key: crate::FuncKey, args: &[Expr]) -> Option<Expr> {
    if let ("value.field", [record, field]) = (output_name(&crate::func_name(key)), args) {
        // Projection preserves the existing first-match domain. Only
        // distribute when every arm has a defined projection; an absent
        // field must not become a null/default or affect another arm.
        fn project(
            record: &Expr,
            field: &Expr,
            key: crate::FuncKey,
            in_case: bool,
        ) -> Option<Expr> {
            if let Some(branches) = record.as_case_when() {
                let projected = branches
                    .iter()
                    .map(|(condition, value)| {
                        Some((
                            condition.clone(),
                            project(value, field, key, true)?,
                        ))
                    })
                    .collect::<Option<Vec<_>>>()?;
                // The input already carries its total/normal-return domain.
                // Rebuild it verbatim, without adding a fallback branch.
                return Some(b::normal_return_case_when(projected));
            }
            if !in_case {
                return None;
            }
            let args = vec![record.clone(), field.clone()];
            Some(b::call(key, args))
        }
        project(record, field, key, false)
    } else {
        None
    }
}
