//! Exact finite partition of a proven integer subject. No sampling of
//! inputs: every comparison endpoint partitions the entire integer domain.
use crate::{builder as b, BinOpKind as B, Expr, Lit};
use num_bigint::BigInt;

// None endpoints are unbounded, not the host word's minimum and maximum.
type Endpoint = Option<BigInt>;
fn is_input_integer(e: &Expr) -> bool {
    matches!(e.as_sym_var(),
        Some(crate::SymVarOrigin::Parameter { name, primitive_type })
            if primitive_type == "integer" && !name.starts_with('$')
                && !name.contains(['.', '[', ']']))
}

#[derive(Clone)]
enum Guard {
    Bool(bool),
    Compare(fn(&BigInt, &BigInt) -> bool, BigInt),
    And(Box<Guard>, Box<Guard>),
    Or(Box<Guard>, Box<Guard>),
}
impl Guard {
    fn at(&self, x: &BigInt) -> bool {
        match self {
            Self::Bool(v) => *v,
            Self::Compare(test, n) => test(x, n),
            Self::And(a, b) => a.at(x) && b.at(x),
            Self::Or(a, b) => a.at(x) || b.at(x),
        }
    }
}
fn guard(e: &Expr, subject: &mut Option<Expr>, cuts: &mut Vec<BigInt>) -> Option<Guard> {
    if let Some(v) = e.as_bool() {
        return Some(Guard::Bool(v));
    }
    let (op, a, z) = e.as_binop()?;
    if matches!(op, B::And | B::Or) {
        let a = Box::new(guard(a, subject, cuts)?);
        let z = Box::new(guard(z, subject, cuts)?);
        return Some(if op == B::And {
            Guard::And(a, z)
        } else {
            Guard::Or(a, z)
        });
    }
    let (op, s, n) = super::constraint::comparison_literal_rhs(
        (op, a, z),
        |expr| expr.as_lit().and_then(Lit::as_integer),
    )?;
    let test = op.comparison_test::<BigInt>()?;
    if !is_input_integer(s) { return None; }
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

fn cells(mut cuts: Vec<BigInt>) -> impl Iterator<Item = (Endpoint, Endpoint)> {
    cuts.sort();
    cuts.dedup();
    let mut start = None;
    cuts.into_iter().map(Some).chain(std::iter::once(None)).map(move |next| {
        let end = next.as_ref().map(|n| n - 1);
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
    if subject.is_none() { return false; }
    cells(cuts).all(|(lo, hi)| {
        let representative = cell_representative(&lo, &hi);
        guards.iter().any(|g| g.at(&representative))
    })
}

pub(super) fn case_ranges(branches: &[(Expr, Expr)]) -> Option<(Vec<(Expr, Expr)>, Expr)> {
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
    let mut segments: Vec<(Endpoint, Endpoint, Expr)> = Vec::new();
    for (start, end) in cells(cuts) {
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
    if let [(_, _, only_value)] = segments.as_slice() {
        return Some((vec![], only_value.clone()));
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
            if let (Some(a), Some(z)) = (a, z) {
                if a == z {
                    intervals.push(b::eq(subject.clone(), b::lit_integer(a.clone())));
                    continue;
                }
            }
            // A segment with no bounds is the sole segment and was
            // returned above. There is no unconditional guard to build.
            let lower = a.as_ref().map(|n| b::ge(subject.clone(), b::lit_integer(n.clone())));
            let upper = z.as_ref().map(|n| b::le(subject.clone(), b::lit_integer(n.clone())));
            intervals.extend(lower.into_iter().chain(upper).reduce(b::and));
        }
        if let Some(domain) = intervals.into_iter().reduce(b::or) {
            out.push((domain, value));
        }
    }
    Some((out, default))
}
