//! Finite position matching over two ordered, typed record sequences.
use std::sync::OnceLock;
use crate::{builder as b, Expr, FuncKey, Lit, Primitive};
use crate::domains::primitive::registry::FuncSemantics;
use super::{relation_count, relation_exists, RelationExpr};

static INDICES: OnceLock<FuncKey> = OnceLock::new();
static MINIMUM: OnceLock<FuncKey> = OnceLock::new();
static INTEGER_FIELD: OnceLock<FuncKey> = OnceLock::new();
static BOOLEAN_FIELD: OnceLock<FuncKey> = OnceLock::new();

fn key(slot: &OnceLock<FuncKey>, name: &str, params: Vec<Primitive>, result: Primitive) -> FuncKey {
    *slot.get_or_init(|| crate::register_func(name, params, Some(result), FuncSemantics::pure_function()))
}

fn indices(upper: Expr) -> Expr {
    let key = key(&INDICES, "indices", vec![Primitive::Number], Primitive::Composite);
    b::call(key, vec![upper])
}

fn minimum(mut a: Expr, mut z: Expr) -> Expr {
    if a == z { return a; }
    let key = key(&MINIMUM, "LEAST", vec![Primitive::Number; 2], Primitive::Number);
    let sort = &crate::DefaultSortContext;
    if crate::canonical_order_key(&a, sort) > crate::canonical_order_key(&z, sort) {
        std::mem::swap(&mut a, &mut z);
    }
    b::call(key, vec![a, z])
}

fn field_at(source: &str, index: Expr, field: &str, ty: Primitive) -> Expr {
    let (slot, name) = match ty {
        Primitive::Number => (&INTEGER_FIELD, "value.ordered_integer_field"),
        Primitive::Boolean => (&BOOLEAN_FIELD, "value.ordered_boolean_field"),
        _ => unreachable!("validated ordered field domain"),
    };
    let key = key(slot, name, vec![Primitive::String, Primitive::Number, Primitive::String], ty);
    b::call(key, vec![b::lit_str(source), index, b::lit_str(field)])
}

pub fn field_projection(expression: &Expr) -> Option<(&str, &Expr, &str, Primitive)> {
    let (key, args) = expression.as_call()?;
    let ty = if INTEGER_FIELD.get() == Some(&key) { Primitive::Number }
        else if BOOLEAN_FIELD.get() == Some(&key) { Primitive::Boolean }
        else { return None; };
    let [source, index, field] = args else { return None; };
    let (Lit::Str(source), Lit::Str(field)) = (source.as_lit()?, field.as_lit()?) else { return None; };
    Some((source, index, field, ty))
}

/// The caller certifies both sequences have the same non-null scalar field contract.
/// The index relation bounds both accesses, including the reverse index.
/// A partial match leaves the stack nonempty; over-close or any mismatch
/// causes the same source exception.
pub fn reverse_stack_completion(opened: &str, closed: &str, field: &str, ty: Primitive) -> (Expr, Expr) {
    let length = |input| relation_count(RelationExpr::base(input, "item"));
    let open_size = length(opened);
    let close_size = length(closed);
    let index = b::row_value("i");
    let reverse = b::sub(b::sub(open_size.clone(), b::lit_int(1)), index.clone());
    let differs = b::ne(field_at(closed, index, field, ty), field_at(opened, reverse, field, ty));
    let common_positions = RelationExpr::vector(indices(minimum(open_size.clone(), close_size.clone())), "i");
    let mismatched = relation_exists(RelationExpr::select(differs, common_positions));
    let empty = b::eq(open_size.clone(), close_size.clone());
    let failure = b::or(b::gt(close_size, open_size), mismatched);
    (empty, failure)
}
