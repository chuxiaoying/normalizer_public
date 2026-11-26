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
        (op.swapped_comparison()?, right, left)
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

/// Closed numeric vector construction. These are immutable values, not
/// JavaScript allocations whose identity or mutation can be observed.
pub fn lifted_number_vector(items: Vec<Expr>) -> Result<Expr, &'static str> {
    use crate::{BinOpKind as Op, SymVarOrigin, UnaryOpKind::Not};

    fn integer(value: &Expr) -> bool {
        if value.as_lit().and_then(crate::Lit::as_integer).is_some() { return true; }
        if let Some(SymVarOrigin::Parameter { name, primitive_type }) = value.as_sym_var() {
            return primitive_type == "integer" && !name.starts_with('$') && !name.contains('.');
        }
        if let Some((arms, fallback)) = value.as_total_case_when() {
            return arms.iter().all(|(guard, arm)| boolean(guard) && integer(arm))
                && integer(fallback);
        }
        false
    }

    fn boolean(value: &Expr) -> bool {
        if value.as_bool().is_some() { return true; }
        if let Some(SymVarOrigin::Parameter { name, primitive_type }) = value.as_sym_var() {
            return primitive_type == "boolean" && !name.starts_with('$') && !name.contains('.');
        }
        match value.as_binop() {
            _ => false,
        }
    }

    fn lift(items: Vec<Expr>, budget: &mut usize) -> Result<Expr, &'static str> {
        *budget = budget.checked_sub(1).ok_or("readonly Vector CASE lift exceeds its expansion budget")?;
        let Some((index, (arms, fallback))) = items.iter().enumerate()
            .find_map(|(index, item)| item.as_total_case_when().map(|parts| (index, parts)))
        else { return Ok(b::array_lit(items)); };
        let mut lift_choice = |choice: &Expr| {
            let mut arm = items.clone();
            arm[index] = choice.clone();
            lift(arm, budget)
        };
        let out = arms.iter().map(|(guard, choice)| {
            Ok((guard.clone(), lift_choice(choice)?))
        }).collect::<Result<Vec<_>, &'static str>>()?;
        Ok(b::case_when(out, lift_choice(fallback)?))
    }
    lift(items, &mut 256).map(crate::normalize)
}

// The constructor is a typed content value, not a JS reference allocation.
static ORDERED_TEXT_MAP: std::sync::OnceLock<crate::FuncKey> = std::sync::OnceLock::new();

fn ordered_text_map(entries: Vec<Expr>) -> Expr {
    let key = *ORDERED_TEXT_MAP.get_or_init(|| crate::register_func(
        "value.ordered_text_map", vec![Primitive::Composite], Some(Primitive::Composite),
        FuncSemantics::pure_function(),
    ));
    b::call(key, vec![b::array_lit(entries)])
}

pub fn ordered_text_map_entries(expr: &Expr) -> Option<&[Expr]> {
    let (key, args) = expr.as_call()?;
    let [entries] = args else { return None; };
    if ORDERED_TEXT_MAP.get() != Some(&key) { return None; }
    entries.as_array_lit()
}

pub fn lifted_ordered_text_map(entries: &[(String, Expr)]) -> Result<Expr, &'static str> {
    use crate::SymVarOrigin;
    let mut keys = std::collections::BTreeSet::new();
    for (key, value) in entries {
        if !keys.insert(key) { return Err("readonly Map requires unique literal keys"); }
    }
    // After seven independent decisions the next level would need 511
    // expansion nodes. Reject before expanding rather than print a fallback.
    if entries.len() > 7 { return Err("readonly Map CASE lift exceeds its expansion budget"); }

    fn lift(entries: &[(String, Expr)], suffix: &mut Vec<Expr>) -> Expr {
        let Some(((key, value), prefix)) = entries.split_last() else {
            return ordered_text_map(suffix.iter().rev().cloned().collect());
        };
        let without = lift(prefix, suffix);
        suffix.push(b::array_lit(vec![b::lit_str(key.as_str()), value.clone()]));
        let with = lift(prefix, suffix);
        suffix.pop();
        b::case_when(vec![(b::eq(value.clone(), b::lit_null()), without)], with)
    }
    Ok(crate::normalize(lift(entries, &mut Vec::new())))
}
