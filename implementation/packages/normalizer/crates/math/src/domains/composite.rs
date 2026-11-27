//! Opaque immutable inputs and their typed projections.
//! The caller must supply a checked structural type.
use super::primitive::registry::FuncSemantics;
use crate::{builder as b, BinOpKind, Expr, Primitive, SymVarOrigin, UnaryOpKind};

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

/// Read a checked field projection without assuming the shape of a generic Call.
pub fn field_projection(expr: &Expr) -> Option<(&Expr, &str, Primitive)> {
    let (key, args) = expr.as_call()?;
    let result = field_intrinsic_result(key)?;
    let [record, field] = args else { return None; };
    let crate::Lit::Str(name) = field.as_lit()? else { return None; };
    Some((record, name.as_str(), result))
}

pub fn field(record: Expr, name: &str, result: Primitive) -> Expr {
    let registered = format!("value.field@{}", result.algebra_name());
    let key = crate::register_func(
        &registered,
        vec![Primitive::Composite, Primitive::String],
        Some(result),
        FuncSemantics::pure_function(),
    );
    field_intrinsics().write().unwrap().insert(key, result);
    crate::normalize(b::call(key, vec![record, b::lit_str(name)]))
}

static VALUE_INPUT: std::sync::OnceLock<crate::FuncKey> = std::sync::OnceLock::new();

pub fn input(name: &str) -> Expr {
    let key = *VALUE_INPUT.get_or_init(|| crate::register_func(
        "value.input", vec![Primitive::String], Some(Primitive::Composite), FuncSemantics::pure_function(),
    ));
    crate::normalize(b::call(key, vec![b::lit_str(name)]))
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
    if VALUE_INPUT.get() != Some(&key) { return None; }
    match args.first()?.as_lit()? { crate::Lit::Str(name) => Some(name), _ => None }
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
    // A finite boundary on this discrete domain has one shared threshold:
    // [0..k] and [k+1..] render as <= k and > k. That keeps the
    // base predicate and its complement identical except for their polarity.
    let (op, n) = match op {
        Ge => (Gt, n - 1),
        Lt => (Le, n - 1),
        _ => (op, n),
    };
    let zero: crate::Integer = 0.into();
    let canonical = match op {
        Gt if n < zero => b::lit_bool(true),
        Le | Eq if n < zero => b::lit_bool(false),
        Ne if n < zero => b::lit_bool(true),
        Gt if n == zero => b::ne(subject.clone(), b::lit_int(0)),
        Le if n == zero => b::eq(subject.clone(), b::lit_int(0)),
        Gt => b::gt(subject.clone(), b::lit_integer(n)),
        Le => b::le(subject.clone(), b::lit_integer(n)),
        _ => return None,
    };
    (canonical != *expr).then_some(canonical)

}

// Only acquired, total values are eligible for changing a Vector's CASE
// geometry. A numeric type alone does not make an E/T, call, or heap
// observation total. This deliberately small proof covers literal and input
// expressions without assigning new meaning to field or receipt reads.
fn total_integer(expr: &Expr) -> bool {
    if expr.as_lit().and_then(crate::Lit::as_integer).is_some() {
        return true;
    }
    if let Some(SymVarOrigin::Parameter { name, primitive_type }) = expr.as_sym_var() {
        return primitive_type == "integer" && direct_parameter(name);
    }
    if let Some((BinOpKind::Add | BinOpKind::Sub | BinOpKind::Mul, left, right)) = expr.as_binop() {
        return total_integer(left) && total_integer(right);
    }
    expr.as_total_case_when().is_some_and(|(arms, fallback)| {
        total_integer(fallback)
            && arms.iter().all(|(guard, value)| total_guard(guard) && total_integer(value))
    })
}

// The host also uses Parameter storage for synthetic observations and field
// paths. Do not mistake those private names for acquired input declarations.
fn direct_parameter(name: &str) -> bool {
    !name.starts_with('$') && !name.contains(['.', '[', ']'])
}

fn total_guard(expr: &Expr) -> bool {
    if expr.as_bool().is_some() {
        return true;
    }
    if let Some(SymVarOrigin::Parameter { name, primitive_type }) = expr.as_sym_var() {
        return primitive_type == "boolean" && direct_parameter(name);
    }
    if let Some((UnaryOpKind::Not, operand)) = expr.as_unary_op() {
        return total_guard(operand);
    }
    match expr.as_binop() {
        Some((BinOpKind::And | BinOpKind::Or, left, right)) => {
            total_guard(left) && total_guard(right)
        }
        Some((op, left, right)) if op.swapped_comparison().is_some() => {
            total_integer(left) && total_integer(right)
        }
        _ => false,
    }
}

fn binary_slot(item: &Expr) -> Option<(&Expr, &Expr, &Expr)> {
    let ([(guard, yes)], no) = item.as_total_case_when()? else {
        return None;
    };
    Some((guard, yes, no))
}

/// Construct a certified immutable numeric Vector in slot order. Independent
/// slot guards stay nested, rather than making a Cartesian product. If the
/// entire decision is shared by multiple slots, one outer CASE instead strictly
/// reduces the primary complexity measure (number of conditional arms).
pub fn number_vector(items: Vec<Expr>) -> Expr {
    if let Some(shared) = shared_vector_choice(&items) {
        return shared;
    }
    b::array_lit(items)
}

fn shared_vector_choice(items: &[Expr]) -> Option<Expr> {
    let mut guard = None;
    let mut varying = 0;
    let mut yes = Vec::with_capacity(items.len());
    let mut no = Vec::with_capacity(items.len());
    for item in items {
        if let Some((candidate, then_value, else_value)) = binary_slot(item) {
            if !total_guard(candidate)
                || ![then_value, else_value].into_iter().all(|value| {
                    !value.has_case_when() && total_integer(value)
                })
            {
                return None;
            }
            let base = *guard.get_or_insert(candidate);
            let (then_value, else_value) = if base == candidate {
                (then_value, else_value)
            } else if crate::normalize(b::not(candidate.clone())) == *base {
                (else_value, then_value)
            } else {
                return None;
            };
            varying += 1;
            yes.push(then_value.clone());
            no.push(else_value.clone());
        } else {
            // A nested conditional decision would be replicated across arms.
            if item.has_case_when() || !total_integer(item) {
                return None;
            }
            yes.push(item.clone());
            no.push(item.clone());
        }
    }
    let guard = guard.filter(|_| varying > 1)?;
    Some(crate::normalize(b::case_when(
        vec![(guard.clone(), b::array_lit(yes))],
        b::array_lit(no),
    )))
}

/// Factor a selection between two actual Vector literals at their only
/// differing slot. The typed caller promises immutable content (not reference
/// identity); this proof additionally rules out partial or effectful operands.
/// Multiple varying slots retain the one shared selector.
pub fn number_vector_choice(guard: &Expr, yes: &Expr, no: &Expr) -> Option<Expr> {
    let (yes, no) = (yes.as_array_lit()?, no.as_array_lit()?);
    if yes.len() != no.len()
        || !total_guard(guard)
        || !yes.iter().chain(no).all(total_integer)
    {
        return None;
    }
    let mut differences = yes.iter().zip(no).enumerate().filter(|(_, (a, b))| a != b);
    let (index, (then_value, else_value)) = differences.next()?;
    // Moving an already nested difference could feed CASE flattening and
    // increase the number of arms. Leave that broader equivalence unclaimed.
    if differences.next().is_some() || then_value.has_case_when() || else_value.has_case_when() {
        return None;
    }
    let mut items = yes.to_vec();
    items[index] = crate::normalize(b::case_when(
        vec![(guard.clone(), then_value.clone())],
        else_value.clone(),
    ));
    Some(b::array_lit(items))
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
    let mut keys = std::collections::BTreeSet::new();
    for (key, _) in entries {
        if !keys.insert(key) { return Err("readonly Map requires unique literal keys"); }
    }
    // Explore the last distinct null decision first, as for source-order
    // insertion. A repeated null test is one decision, not another product
    // axis. A constant null check consumes no decision at all.
    let mut decisions = Vec::<Expr>::new();
    let mut selectors = Vec::with_capacity(entries.len());
    for (_, value) in entries.iter().rev() {
        let guard = crate::normalize(b::eq(value.clone(), b::lit_null()));
        selectors.push(if let Some(absent) = guard.as_bool() {
            Presence::Known(!absent)
        } else if let Some(index) = decisions.iter().position(|known| *known == guard) {
            Presence::Decision(index)
        } else {
            if decisions.len() == 7 {
                return Err("readonly Map CASE lift exceeds its expansion budget");
            }
            let index = decisions.len();
            decisions.push(guard);
            Presence::Decision(index)
        });
    }
    selectors.reverse();

    #[derive(Clone, Copy)]
    enum Presence {
        Known(bool),
        Decision(usize),
    }

    fn lift(
        entries: &[(String, Expr)],
        selectors: &[Presence],
        decisions: &[Expr],
        depth: usize,
        present: u8,
    ) -> Expr {
        let Some(guard) = decisions.get(depth) else {
            let rows = entries.iter().zip(selectors).filter_map(|((key, value), selector)| {
                let include = match selector {
                    Presence::Known(include) => *include,
                    Presence::Decision(index) => present & (1 << index) != 0,
                };
                include.then(|| b::array_lit(vec![b::lit_str(key.as_str()), value.clone()]))
            });
            return ordered_text_map(rows.collect());
        };
        let without = lift(entries, selectors, decisions, depth + 1, present);
        let with = lift(entries, selectors, decisions, depth + 1, present | (1 << depth));
        b::case_when(vec![(guard.clone(), without)], with)
    }
    Ok(crate::normalize(lift(entries, &selectors, &decisions, 0, 0)))
}
