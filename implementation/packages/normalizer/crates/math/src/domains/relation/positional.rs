//! Finite integer domains and projections over acquired sequences.
use std::sync::OnceLock;
use crate::{builder as b, BinOpKind, Expr, FuncKey, Lit, Primitive};
use crate::domains::integer_quotient::on_nonzero_path;
use crate::domains::primitive::registry::FuncSemantics;
use super::RelationExpr;

static RANGE: OnceLock<FuncKey> = OnceLock::new();
static INDICES: OnceLock<FuncKey> = OnceLock::new();
static MINIMUM: OnceLock<FuncKey> = OnceLock::new();
static INTEGER_FIELD: OnceLock<FuncKey> = OnceLock::new();
static BOOLEAN_FIELD: OnceLock<FuncKey> = OnceLock::new();

fn key(slot: &OnceLock<FuncKey>, name: &str, params: Vec<Primitive>, result: Primitive) -> FuncKey {
    *slot.get_or_init(|| crate::register_func(name, params, Some(result), FuncSemantics::pure_function()))
}

pub fn indices(upper: Expr) -> Expr {
    let key = key(&INDICES, "indices", vec![Primitive::Number], Primitive::Composite);
    b::call(key, vec![upper])
}

pub fn range(lower: Expr, upper: Expr) -> Expr {
    if lower.as_lit().and_then(Lit::as_integer).is_some_and(|n| n == 0.into()) { return indices(upper); }
    let key = key(&RANGE, "range", vec![Primitive::Number; 2], Primitive::Composite);
    b::call(key, vec![lower, upper])
}

pub fn minimum(mut a: Expr, mut z: Expr) -> Expr {
    if a == z { return a; }
    let key = key(&MINIMUM, "LEAST", vec![Primitive::Number; 2], Primitive::Number);
    let sort = &crate::DefaultSortContext;
    if crate::canonical_order_key(&a, sort) > crate::canonical_order_key(&z, sort) {
        std::mem::swap(&mut a, &mut z);
    }
    b::call(key, vec![a, z])
}

pub fn field_at(source: &str, index: Expr, field: &str, ty: Primitive) -> Expr {
    let (slot, name) = match ty {
        Primitive::Number => (&INTEGER_FIELD, "value.ordered_integer_field"),
        Primitive::Boolean => (&BOOLEAN_FIELD, "value.ordered_boolean_field"),
        _ => unreachable!("validated ordered field domain"),
    };
    let key = key(slot, name, vec![Primitive::String, Primitive::Number, Primitive::String], ty);
    b::call(key, vec![b::lit_str(source), index, b::lit_str(field)])
}

pub fn range_bounds(expression: &Expr) -> Option<(Option<&Expr>, &Expr)> {
    let (key, args) = expression.as_call()?;
    if INDICES.get() == Some(&key) {
        let [upper] = args else { return None; };
        return Some((None, upper));
    }
    if RANGE.get() == Some(&key) {
        let [lower, upper] = args else { return None; };
        return Some((Some(lower), upper));
    }
    None
}

pub fn minimum_bounds(expression: &Expr) -> Option<(&Expr, &Expr)> {
    let (key, args) = expression.as_call()?;
    let [left, right] = args else { return None; };
    (MINIMUM.get() == Some(&key)).then_some((left, right))
}

pub fn integer_domain(relation: &RelationExpr) -> Option<(&Expr, &str)> {
    let mut source = relation;
    while let RelationExpr::Select { input, .. } = source { source = input; }
    let RelationExpr::Vector { value, alias } = source else { return None; };
    range_bounds(value)?;
    Some((value, alias))
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

fn known_nonnegative(value: &Expr, active: &Expr) -> bool {
    if active.as_bool() == Some(false) { return true; }
    if let Some(number) = value.as_lit().and_then(Lit::as_integer) {
        return number >= 0.into();
    }
    if value.as_relation_aggregate().is_some_and(|(kind, _, _, _)|
        kind == crate::RelationAggregateKind::Count)
    {
        return true;
    }
    if let Some((arms, otherwise)) = value.as_total_case_when() {
        let mut remaining = active.clone();
        for (test, arm) in arms {
            let (domain, later) = b::first_match_domains(remaining, test);
            if !known_nonnegative(arm, &domain) { return false; }
            remaining = later;
        }
        return known_nonnegative(otherwise, &remaining);
    }
    b::path_implies(active, &b::ge(value.clone(), b::lit_int(0)))
}

pub fn count(lower: Expr, upper: Expr) -> Expr {
    // A zero-based integer interval ending at a proven nonnegative
    // value contains exactly that many integers (RA-11). In particular,
    // do not introduce a second clamp around a closed MAX.
    if lower.as_lit().and_then(Lit::as_integer).is_some_and(|n| n == 0.into())
        && known_nonnegative(&upper, &b::lit_bool(true))
    {
        return upper;
    }
    let length = crate::normalize(b::sub(upper, lower));
    b::case_when(vec![(b::gt(length.clone(), b::lit_int(0)), length)], b::lit_int(0))
}

pub fn interval_sum(lower: Expr, upper: Expr) -> Expr {
    // Let a clipped endpoint contribute one formula per distinct bound,
    // rather than one result per integer in the interval.
    if let Some((arms, end)) = upper.as_total_case_when() {
        return b::case_when(arms.iter().map(|(guard, limit)|
            (guard.clone(), interval_sum(lower.clone(), limit.clone()))).collect(),
            interval_sum(lower, end.clone()));
    }
    let length = crate::normalize(b::sub(upper.clone(), lower.clone()));
    let two_times_mean = b::sub(b::add(upper.clone(), lower.clone()), b::lit_int(1));
    let total = on_nonzero_path(crate::normalize(b::mul(length, two_times_mean)), b::lit_int(2));
    crate::normalize(b::case_when(vec![(b::lt(lower, upper), total)], b::lit_int(0)))
}

// Only the declared straight-line arithmetic contributes to this proof.
// Unsupported expressions remain ordinary aggregate nodes, not an execution
// loop. Array projections and predicates are handled by the relation IR.
pub fn integer_polynomial(value: &Expr, row: &str) -> Option<[Expr; 3]> {
    let zero = || b::lit_int(0);
    if value.as_row_value() == Some(row) {
        return Some([zero(), b::lit_int(1), zero()]);
    }
    let mut depends = false;
    value.walk(&mut |part| depends |= part.as_row_value() == Some(row));
    if !depends { return Some([value.clone(), zero(), zero()]); }
    let (op, lhs, rhs) = value.as_binop()?;
    let left = integer_polynomial(lhs, row)?;
    let right = integer_polynomial(rhs, row)?;
    let combine = match op {
        BinOpKind::Add => b::add,
        BinOpKind::Sub => b::sub,
        BinOpKind::Mul => {
            let mut coefficients = [zero(), zero(), zero()];
            for (a, l) in left.iter().enumerate() {
                for (z, r) in right.iter().enumerate() {
                    let term = crate::normalize(b::mul(l.clone(), r.clone()));
                    if term == zero() { continue; }
                    let output = coefficients.get_mut(a + z)?;
                    *output = crate::normalize(b::add(output.clone(), term));
                }
            }
            return Some(coefficients);
        }
        _ => return None,
    };
    Some(std::array::from_fn(|i| crate::normalize(combine(left[i].clone(), right[i].clone()))))
}

fn sum_squares(lower: Expr, upper: Expr) -> Expr {
    let before = |n: Expr| {
        let scale = b::sub(b::mul(b::lit_int(2), n.clone()), b::lit_int(1));
        let product = b::mul(b::mul(n.clone(), b::sub(n, b::lit_int(1))), scale);
        on_nonzero_path(crate::normalize(product), b::lit_int(6))
    };
    let guard = b::lt(lower.clone(), upper.clone());
    b::case_when(vec![(guard, b::sub(before(upper), before(lower)))], b::lit_int(0))
}

pub fn sum_polynomial(coefficients: [Expr; 3], lower: Expr, upper: Expr) -> Expr {
    let [c0, c1, c2] = coefficients;
    let mut terms = b::lit_int(0);
    if c0 != b::lit_int(0) {
        terms = b::add(terms, b::mul(c0, count(lower.clone(), upper.clone())));
    }
    if c1 != b::lit_int(0) {
        terms = b::add(terms, b::mul(c1, interval_sum(lower.clone(), upper.clone())));
    }
    if c2 != b::lit_int(0) {
        terms = b::add(terms, b::mul(c2, sum_squares(lower, upper)));
    }
    terms
}
