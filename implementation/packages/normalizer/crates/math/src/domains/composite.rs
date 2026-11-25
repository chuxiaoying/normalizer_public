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
