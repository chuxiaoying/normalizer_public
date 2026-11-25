//! Exact finite partition of a proven bounded integer subject. No sampling of
//! inputs: every comparison endpoint partitions the entire integer domain.
use crate::{builder as b, BinOpKind as B, Expr, Lit};

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
        "byte" => Some((i8::MIN as i128, i8::MAX as i128)),
        "short" => Some((i16::MIN as i128, i16::MAX as i128)),
        "int" => Some((i32::MIN as i128, i32::MAX as i128)),
        "long" => Some((i64::MIN as i128, i64::MAX as i128)),
        "char" => Some((0, u16::MAX as i128)),
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

#[derive(Clone)]
enum Guard {
    Bool(bool),
    Compare(B, i128),
    And(Box<Guard>, Box<Guard>),
    Or(Box<Guard>, Box<Guard>),
    Not(Box<Guard>),
}
impl Guard {
    fn at(&self, x: i128) -> bool {
        match self {
            Self::Bool(v) => *v,
            Self::Compare(op, n) => match op {
                B::Eq => x == *n,
                B::Ne => x != *n,
                B::Lt => x < *n,
                B::Le => x <= *n,
                B::Gt => x > *n,
                B::Ge => x >= *n,
                _ => unreachable!(),
            },
            Self::And(a, b) => a.at(x) && b.at(x),
            Self::Or(a, b) => a.at(x) || b.at(x),
            Self::Not(a) => !a.at(x),
        }
    }
}
fn guard(e: &Expr, subject: &mut Option<Expr>, cuts: &mut Vec<i128>) -> Option<Guard> {
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
    if !matches!(op, B::Eq | B::Ne | B::Lt | B::Le | B::Gt | B::Ge) {
        return None;
    }
    let (s, n) = if let Some(Lit::Int(n)) = z.as_lit() {
        (a, *n)
    } else if let Some(Lit::Int(n)) = a.as_lit() {
        op = match op {
            B::Lt => B::Gt,
            B::Le => B::Ge,
            B::Gt => B::Lt,
            B::Ge => B::Le,
            other => other,
        };
        (z, *n)
    } else {
        return None;
    };
    bounds(s)?;
    if let Some(old) = subject {
        if old != s {
            return None;
        }
    } else {
        *subject = Some(s.clone());
    }
    cuts.push(n as i128);
    cuts.push(n as i128 + 1);
    Some(Guard::Compare(op, n as i128))
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
    let (lo, hi) = bounds(&subject)?;
    cuts.retain(|n| *n > lo && *n <= hi);
    cuts.extend([lo, hi + 1]);
    cuts.sort();
    cuts.dedup();
    let mut segments: Vec<(i128, i128, Expr)> = vec![];
    for pair in cuts.windows(2) {
        let value = branches[guards.iter().position(|g| g.at(pair[0]))?]
            .1
            .clone();
        if let Some(last) = segments.last_mut() {
            if last.2 == value {
                last.1 = pair[1] - 1;
                continue;
            }
        }
        segments.push((pair[0], pair[1] - 1, value));
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
            let condition = if a == z {
                b::eq(subject.clone(), b::lit_int(*a as i64))
            } else if *a == lo && *z == hi {
                b::lit_bool(true)
            } else if *a == lo {
                b::le(subject.clone(), b::lit_int(*z as i64))
            } else if *z == hi {
                b::ge(subject.clone(), b::lit_int(*a as i64))
            } else {
                b::and(
                    b::ge(subject.clone(), b::lit_int(*a as i64)),
                    b::le(subject.clone(), b::lit_int(*z as i64)),
                )
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
