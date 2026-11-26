//! Exact finite partition of a proven integer subject. No sampling of
//! inputs: every comparison endpoint partitions the entire integer domain.
use crate::{builder as b, BinOpKind as B, Expr, Lit};
use num_bigint::BigInt;

// Only direct acquired entry parameters carry this declaration evidence.
// Legacy expansion also encodes partial field/E projections as Parameter paths;
// numeric annotations on those paths do not certify a total bounded input.
fn native_parameter_bounds(e: &Expr) -> Option<(i128, i128)> {
    let crate::expr::ExprKind::SymVar {
        origin:
            crate::symvar::SymVarOrigin::Parameter {
                name,
                primitive_type,
            },
    } = e.kind()
    else {
        return None;
    };
    if name.starts_with("$e_")
        || name.starts_with("$t_")
        || name.starts_with("$h_")
        || name.contains(['.', '[', ']'])
    {
        return None;
    }
    match primitive_type.as_str() {
        "i32" => Some((i32::MIN as i128, i32::MAX as i128)),
        "i64" => Some((i64::MIN as i128, i64::MAX as i128)),
        _ => None,
    }
}

pub(super) fn bounds(e: &Expr) -> Option<(i128, i128)> {
    if let Some(domain) = native_parameter_bounds(e) {
        return Some(domain);
    }
    if let Some(Lit::Int(n)) = e.as_lit() {
        return Some((*n as i128, *n as i128));
    }
    None
}

// None endpoints are unbounded, not the host word's minimum and maximum.
type Endpoint = Option<BigInt>;
fn subject_domain(e: &Expr) -> Option<(Endpoint, Endpoint)> {
    if let Some((lo, hi)) = bounds(e) {
        return Some((Some(lo.into()), Some(hi.into())));
    }
    match e.as_sym_var() {
        Some(crate::SymVarOrigin::Parameter { name, primitive_type })
            if primitive_type == "integer" && !name.starts_with('$')
                && !name.contains(['.', '[', ']']) => Some((None, None)),
        _ => None,
    }
}

#[derive(Clone)]
enum Guard {
    Bool(bool),
    Compare(fn(&BigInt, &BigInt) -> bool, BigInt),
    And(Box<Guard>, Box<Guard>),
    Or(Box<Guard>, Box<Guard>),
    Not(Box<Guard>),
}
impl Guard {
    fn at(&self, x: &BigInt) -> bool {
        match self {
            Self::Bool(v) => *v,
            Self::Compare(test, n) => test(x, n),
            Self::And(a, b) => a.at(x) && b.at(x),
            Self::Or(a, b) => a.at(x) || b.at(x),
            Self::Not(a) => !a.at(x),
        }
    }
}
fn guard(e: &Expr, subject: &mut Option<Expr>, cuts: &mut Vec<BigInt>) -> Option<Guard> {
    if let Some(v) = e.as_bool() {
        return Some(Guard::Bool(v));
    }
    if let Some((crate::UnaryOpKind::Not, v)) = e.as_unary_op() {
        return Some(Guard::Not(Box::new(guard(v, subject, cuts)?)));
    }
    let (mut op, a, z) = e.as_binop()?;
    if matches!(op, B::And | B::Or) {
        let a = Box::new(guard(a, subject, cuts)?);
        let z = Box::new(guard(z, subject, cuts)?);
        return Some(if op == B::And {
            Guard::And(a, z)
        } else {
            Guard::Or(a, z)
        });
    }
    let (s, n) = if let Some(n) = z.as_lit().and_then(Lit::as_integer) {
        (a, n)
    } else if let Some(n) = a.as_lit().and_then(Lit::as_integer) {
        op = op.swapped_comparison()?;
        (z, n)
    } else {
        return None;
    };
    let test: fn(&BigInt, &BigInt) -> bool = match op {
        B::Eq => BigInt::eq,
        B::Ne => BigInt::ne,
        B::Lt => BigInt::lt,
        B::Le => BigInt::le,
        B::Gt => BigInt::gt,
        B::Ge => BigInt::ge,
        _ => return None,
    };
    subject_domain(s)?;
    if let Some(old) = subject {
        if old != s {
            return None;
        }
    } else {
        *subject = Some(s.clone());
    }
    cuts.push(n.clone());
    cuts.push(&n + 1);
    Some(Guard::Compare(test, n))
}

fn cells(lo: Endpoint, hi: Endpoint, mut cuts: Vec<BigInt>) -> impl Iterator<Item = (Endpoint, Endpoint)> {
    cuts.retain(|n| lo.as_ref().is_none_or(|v| n > v)
        && hi.as_ref().is_none_or(|v| n <= v));
    cuts.sort();
    cuts.dedup();
    let mut start = lo;
    cuts.into_iter().map(Some).chain(std::iter::once(None)).map(move |next| {
        let end = next.as_ref().map(|n| n - 1).or_else(|| hi.clone());
        (std::mem::replace(&mut start, next), end)
    })
}

// Every comparison is constant on a cell. This is exact enumeration of
// equivalence classes, including the two possible unbounded tail cells.
fn cell_representative(lo: &Endpoint, hi: &Endpoint) -> BigInt {
    lo.as_ref().or(hi.as_ref()).cloned().unwrap_or_default()
}

/// Prove a small union on one certified integer domain, without assuming
/// totality for fields, nullable values, call results or other subjects.
pub(super) fn exhaustive_union(disjuncts: &[Expr]) -> bool {
    if !(3..=16).contains(&disjuncts.len()) || disjuncts.iter().map(Expr::size).sum::<usize>() > 64 {
        return false;
    }
    let mut subject = None;
    let mut cuts = Vec::new();
    let Some(guards) = disjuncts.iter().map(|c| guard(c, &mut subject, &mut cuts))
        .collect::<Option<Vec<_>>>() else { return false; };
    let Some((lo, hi)) = subject.as_ref().and_then(subject_domain) else { return false; };
    cells(lo, hi, cuts).all(|(lo, hi)| {
        let representative = cell_representative(&lo, &hi);
        guards.iter().any(|g| g.at(&representative))
    })
}

pub(super) fn case_ranges(branches: &[(Expr, Expr)]) -> Option<Expr> {
    if branches.len() < 3
        || !branches.last()?.0.as_bool().is_some_and(|b| b)
        || branches.iter().any(|(_, v)| v.as_case_when().is_some())
    {
        return None;
    }
    let mut subject = None;
    let mut cuts = vec![];
    let guards = branches
        .iter()
        .map(|(c, _)| guard(c, &mut subject, &mut cuts))
        .collect::<Option<Vec<_>>>()?;
    let subject = subject?;
    let (lo, hi) = subject_domain(&subject)?;
    let mut segments: Vec<(Endpoint, Endpoint, Expr)> = Vec::new();
    for (start, end) in cells(lo.clone(), hi.clone(), cuts) {
        let representative = cell_representative(&start, &end);
        let value = branches[guards.iter().position(|g| g.at(&representative))?].1.clone();
        if let Some(last) = segments.last_mut() {
            if last.2 == value {
                last.1 = end;
                continue;
            }
        }
        segments.push((start, end, value));
    }
    // Keep the explicit default and the source's first occurrence of each
    // non-default payload. Partition cells determine effective domains, not a
    // new payload order. In particular do not split a previously grouped
    // payload back into several CASE arms: that would invert same-value
    // grouping on every other normalization iteration.
    let default = branches.last()?.1.clone();
    let mut payloads: Vec<Expr> = vec![];
    for (_, value) in branches {
        if value != &default && !payloads.contains(value) {
            payloads.push(value.clone());
        }
    }
    let mut out = vec![];
    for value in payloads {
        let mut intervals = vec![];
        for (a, z, cell_value) in &segments {
            if cell_value != &value {
                continue;
            }
            let condition = match (a, z) {
                (Some(a), Some(z)) if a == z => b::eq(subject.clone(), b::lit_integer(a.clone())),
                (a, z) => {
                    let lower = a.as_ref().filter(|_| a != &lo)
                        .map(|n| b::ge(subject.clone(), b::lit_integer(n.clone())));
                    let upper = z.as_ref().filter(|_| z != &hi)
                        .map(|n| b::le(subject.clone(), b::lit_integer(n.clone())));
                    match (lower, upper) {
                        (None, None) => b::lit_bool(true),
                        (Some(term), None) | (None, Some(term)) => term,
                        (Some(lower), Some(upper)) => b::and(lower, upper),
                    }
                }
            };
            intervals.push(condition);
        }
        if let Some(domain) = intervals.into_iter().reduce(b::or) {
            out.push((domain, value));
        }
    }
    out.push((b::lit_bool(true), default));
    Some(b::case_when(out))
}
