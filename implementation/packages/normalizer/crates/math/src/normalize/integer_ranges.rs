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

// Unlike a lower-bound proof, this establishes the domain of the
// evaluated expression, not whether it is nonnegative or always acquired.
fn has_exact_integer_domain(value: &Expr) -> bool {
    if super::integer_ac::total_integer(value) { return true; }
    if crate::domains::composite::vector_length_input(value).is_some() { return true; }
    if let Some((kind, _, projection, _)) = value.as_relation_aggregate() {
        use crate::RelationAggregateKind::*;
        return kind == Count || kind == Sum && projection.is_some_and(has_exact_integer_domain);
    }
    if let Some((B::Add | B::Sub | B::Mul, left, right)) = value.as_binop() {
        return has_exact_integer_domain(left) && has_exact_integer_domain(right);
    }
    false
}

/// CF-07's strict representative for one exact integer bound. Keeping
/// the acquired value once and changing only the literal cannot move a read.
pub(super) fn strict_endpoint(expr: &Expr) -> Option<Expr> {
    let (op, subject, limit) = super::constraint::comparison_literal_rhs(
        expr.as_binop()?, |value| value.as_lit().and_then(Lit::as_integer))?;
    if !has_exact_integer_domain(subject) { return None; }
    // Preserve the existing zero/empty representative for an exact count.
    if op == B::Le && limit == BigInt::from(0)
        && subject.as_relation_aggregate().is_some_and(|(kind, _, projection, _)|
            kind == crate::RelationAggregateKind::Count && projection.is_none())
    { return None; }
    match op {
        B::Ge => Some(b::gt(subject.clone(), b::lit_integer(limit - 1))),
        B::Le => Some(b::lt(subject.clone(), b::lit_integer(limit + 1))),
        _ => None,
    }
}

#[derive(Clone)]
enum Guard {
    Bool(bool),
    Compare(fn(&BigInt, &BigInt) -> bool, BigInt),
    Not(Box<Guard>),
    And(Box<Guard>, Box<Guard>),
    Or(Box<Guard>, Box<Guard>),
}
impl Guard {
    fn at(&self, x: &BigInt) -> bool {
        match self {
            Self::Bool(v) => *v,
            Self::Compare(test, n) => test(x, n),
            Self::Not(guard) => !guard.at(x),
            Self::And(a, b) => a.at(x) && b.at(x),
            Self::Or(a, b) => a.at(x) || b.at(x),
        }
    }
}
fn guard(e: &Expr, subject: &mut Option<Expr>, cuts: &mut Vec<BigInt>) -> Option<Guard> {
    guard_with_acquisitions(e, subject, cuts, false)
}

fn guard_with_acquisitions(
    e: &Expr, subject: &mut Option<Expr>, cuts: &mut Vec<BigInt>, acquired_inputs: bool,
) -> Option<Guard> {
    if let Some(v) = e.as_bool() {
        return Some(Guard::Bool(v));
    }
    if let Some(operand) = e.as_not() {
        return Some(Guard::Not(Box::new(guard_with_acquisitions(operand, subject, cuts, acquired_inputs)?)));
    }
    let (op, a, z) = e.as_binop()?;
    if matches!(op, B::And | B::Or) {
        let a = Box::new(guard_with_acquisitions(a, subject, cuts, acquired_inputs)?);
        let z = Box::new(guard_with_acquisitions(z, subject, cuts, acquired_inputs)?);
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
    let closed_length = match s.as_relation_aggregate() {
        Some((crate::RelationAggregateKind::Count,
            crate::domains::relation::RelationExpr::Base { snapshot: None, .. }, None, None)) => true,
        _ => crate::domains::composite::vector_length_input(s).is_some_and(is_input_integer_vector),
    };
    let acquired_input = acquired_inputs && matches!(s.as_sym_var(),
        Some(crate::SymVarOrigin::Parameter { primitive_type, .. }) if primitive_type == "integer");
    if !is_input_integer(s) && !closed_length && !acquired_input { return None; }
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

/// A source guard over one acquired, exact integer parameter is total.
pub(super) fn is_closed_integer_guard(expr: &Expr) -> bool {
    closed_integer_subject(expr).is_some()
}

pub(super) fn closed_integer_subject(expr: &Expr) -> Option<Expr> {
    let mut subject = None;
    guard(expr, &mut subject, &mut Vec::new())?;
    subject
}

fn is_input_integer_vector(value: &Expr) -> bool {
    // The vector's immutable input binding, not a guarded receipt or
    // an arbitrary expression, supplies its already acquired length.
    crate::domains::composite::input_name(value)
        .is_some_and(|name| !name.starts_with('$') && !name.contains(['.', '[', ']']))
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

/// Two strict comparisons may partition one already-evaluated exact
/// input value even if that value was read through a protected field path.
/// This certifies only the two tests of the same value; it does not treat
/// arbitrary fields as globally available to the range solver.
pub(super) fn acquired_integer_strict_bound(expr: &Expr) -> Option<(B, &Expr, BigInt)> {
    let (op, value, limit) = super::constraint::comparison_literal_rhs(
        expr.as_binop()?, |rhs| rhs.as_lit().and_then(Lit::as_integer))?;
    (matches!(op, B::Gt | B::Lt)
        && matches!(value.as_sym_var(), Some(crate::SymVarOrigin::Parameter { primitive_type, .. })
            if primitive_type == "integer")).then_some((op, value, limit))
}

pub(super) fn adjacent_acquired_integer_bounds(left: &Expr, right: &Expr) -> bool {
    let (Some((a_op, a, m)), Some((b_op, z, n))) =
        (acquired_integer_strict_bound(left), acquired_integer_strict_bound(right)) else { return false; };
    if a != z { return false; }
    match (a_op, b_op) {
        (B::Gt, B::Lt) => m + 1 == n,
        (B::Lt, B::Gt) => n + 1 == m,
        _ => false,
    }
}

/// Strict integer boundaries can be complementary at adjacent endpoints.
/// This proof never uses a displayed LENGTH/name as a type certificate.
pub(super) fn complementary_guards(left: &Expr, right: &Expr) -> bool {
    if adjacent_acquired_integer_bounds(left, right) { return true; }
    let mut subject = None;
    let mut cuts = Vec::new();
    let Some(left) = guard(left, &mut subject, &mut cuts) else { return false; };
    let Some(right) = guard(right, &mut subject, &mut cuts) else { return false; };
    if subject.is_none() { return false; }
    cells(cuts).all(|(low, high)| {
        let point = cell_representative(&low, &high);
        left.at(&point) != right.at(&point)
    })
}

/// Prove a small OR union over already-evaluated exact integer
/// predicates. This is a predicate identity, not a proof that an
/// unselected CASE arm's underlying source read is available.
pub(super) fn exhaustive_union(disjuncts: &[Expr]) -> bool {
    if !(3..=16).contains(&disjuncts.len()) || disjuncts.iter().map(Expr::size).sum::<usize>() > 64 {
        return false;
    }
    struct Partition {
        subject: Expr,
        guards: Vec<Guard>,
        cuts: Vec<BigInt>,
    }
    let mut groups = Vec::<Partition>::new();
    for disjunct in disjuncts {
        let mut subject = None;
        let mut cuts = Vec::new();
        let Some(guard) = guard_with_acquisitions(disjunct, &mut subject, &mut cuts, true) else { continue; };
        let Some(subject) = subject else { continue; };
        if let Some(group) = groups.iter_mut().find(|group| group.subject == subject) {
            group.guards.push(guard);
            group.cuts.extend(cuts);
        } else {
            groups.push(Partition { subject, guards: vec![guard], cuts });
        }
    }
    groups.into_iter().any(|group| {
        group.guards.len() >= 3 && cells(group.cuts).all(|(lo, hi)| {
            let point = cell_representative(&lo, &hi);
            group.guards.iter().any(|guard| guard.at(&point))
        })
    })
}

/// The suffix remains after all earlier CASE guards. Only a complete
/// partition on the same exact value can replace its last guard with ELSE.
pub(super) fn exhaustive_acquired_suffix(branches: &[(Expr, Expr)]) -> bool {
    const MAX_SIZE: usize = 64;
    let mut subject = None;
    let mut cuts = Vec::new();
    let mut guards = Vec::new();
    let mut size = 0;
    for (condition, _) in branches.iter().rev().take(16) {
        size += condition.size();
        if size > MAX_SIZE { break; }
        let Some(guard) = guard_with_acquisitions(condition, &mut subject, &mut cuts, true) else { break; };
        guards.push(guard);
    }
    subject.is_some() && guards.len() >= 3 && cells(cuts).all(|(lo, hi)| {
        let point = cell_representative(&lo, &hi);
        guards.iter().any(|guard| guard.at(&point))
    })
}

/// CF-07 on the domain of one already acquired integer parameter.
pub(super) fn canonical_guard(
    expr: &Expr, parent: Option<&Expr>, ctx: &dyn crate::SortContext,
) -> Option<Expr> {
    // At the root, isolated bounds already go through strict_endpoint.
    // A path domain can expose a shorter representative even for one bound.
    let compound = expr.as_not().is_some()
        || matches!(expr.as_binop(), Some((B::And | B::Or, _, _)));
    if parent.is_none() && !compound { return None; }

    let mut subject = None;
    let mut cuts = Vec::new();
    let predicate = guard(expr, &mut subject, &mut cuts)?;
    let subject = subject?;
    let domain = match parent {
        Some(parent) => {
            // Only use a domain whose truth is proved on this same subject.
            // A domain using other observations does not certify a projection.
            guard(parent, &mut Some(subject.clone()), &mut cuts)?
        }
        None => Guard::Bool(true),
    };
    let input_nodes = expr.size().saturating_add(parent.map_or(0, Expr::size));
    if !super::require_candidate_budget(
        cuts.len().saturating_add(1).saturating_mul(input_nodes), input_nodes,
    ) { return None; }

    struct Segment { low: Endpoint, high: Endpoint, truth: bool }
    let mut parts: Vec<Segment> = Vec::new();
    for (low, high) in cells(cuts) {
        let representative = cell_representative(&low, &high);
        if !domain.at(&representative) { continue; }
        let truth = predicate.at(&representative);
        if let Some(last) = parts.last_mut() {
            // Holes outside the domain do not split equal truth segments.
            if last.truth == truth {
                last.high = high;
                continue;
            }
        }
        parts.push(Segment { low, high, truth });
    }
    if parts.is_empty() { return None; }
    if parts.len() == 1 { return Some(b::lit_bool(parts[0].truth)); }

    let point = |segment: &Segment| segment.low.as_ref().zip(segment.high.as_ref())
        .filter(|(low, high)| low == high).map(|(low, _)| low.clone());
    let sort = |items: &mut Vec<Expr>| {
        items.sort_by_cached_key(|value| crate::canonical_order_key(value, ctx));
    };
    let mut union = Vec::new();
    for (index, segment) in parts.iter().enumerate() {
        if !segment.truth { continue; }
        if let Some(value) = point(segment) {
            union.push(b::eq(subject.clone(), b::lit_integer(value)));
            continue;
        }
        let mut span = Vec::new();
        if index != 0 {
            span.push(b::gt(subject.clone(), b::lit_integer(segment.low.as_ref()? - 1)));
        }
        if index != parts.len() - 1 {
            span.push(b::lt(subject.clone(), b::lit_integer(segment.high.as_ref()? + 1)));
        }
        sort(&mut span);
        union.push(super::logical_chain::rebuild_and(&span));
    }
    sort(&mut union);
    let union = super::logical_chain::rebuild_or(&union);

    // Also exclude each maximal false segment. This retains the compact
    // conjunction when several isolated points (or bounded spans) are
    // excluded, rather than expanding it to a union of many true spans.
    let mut intersection = Vec::new();
    for (index, segment) in parts.iter().enumerate() {
        if segment.truth { continue; }
        let interior = index != 0 && index != parts.len() - 1;
        if let Some(value) = point(segment).filter(|_| interior) {
            intersection.push(b::ne(subject.clone(), b::lit_integer(value)));
            continue;
        }
        let mut outside = Vec::new();
        if index != 0 {
            outside.push(b::lt(subject.clone(), b::lit_integer(segment.low.as_ref()?.clone())));
        }
        if index != parts.len() - 1 {
            outside.push(b::gt(subject.clone(), b::lit_integer(segment.high.as_ref()?.clone())));
        }
        sort(&mut outside);
        intersection.push(super::logical_chain::rebuild_or(&outside));
    }
    sort(&mut intersection);
    let intersection = super::logical_chain::rebuild_and(&intersection);
    // Alternate exclusion spelling is a simplification only when cheaper.
    // At equal cost keep the endpoints derived from the true segments.
    let selected = if crate::canonical_order::canonical_cost_compare(&intersection, &union, ctx).is_lt() {
        intersection
    } else { union };
    // This family of representatives must not obstruct a separately proven,
    // strictly smaller formula; compare with the same CF-02 cost.
    (!crate::canonical_order::canonical_cost_compare(&selected, expr, ctx).is_gt()).then_some(selected)
}

/// Determine which extremum the original complete CASE computes, even
/// when its guard uses a shifted integer threshold. At the literal itself
/// either arm is valid. Do not identify an extremum on an empty domain.
pub(crate) fn integer_selection(
    condition: &Expr, subject: &Expr, point: &BigInt, when_true_is_subject: bool, parent: Option<&Expr>,
) -> Option<B> {
    if !is_input_integer(subject) { return None; }
    let mut input = Some(subject.clone());
    let mut cuts = vec![point.clone(), point + 1];
    let predicate = guard(condition, &mut input, &mut cuts)?;
    let domain = match parent {
        Some(parent) => guard(parent, &mut input, &mut cuts)?,
        None => Guard::Bool(true),
    };
    let nodes = condition.size().saturating_add(parent.map_or(0, Expr::size));
    if !super::require_candidate_budget(cuts.len().saturating_add(1).saturating_mul(nodes), nodes) {
        return None;
    }
    let (mut maximum, mut minimum, mut saw_difference) = (true, true, false);
    for (lo, hi) in cells(cuts) {
        let at = cell_representative(&lo, &hi);
        if !domain.at(&at) || &at == point { continue; }
        saw_difference = true;
        let selected_subject = predicate.at(&at) == when_true_is_subject;
        maximum &= selected_subject == (&at > point);
        minimum &= selected_subject == (&at < point);
    }
    match (saw_difference, maximum, minimum) {
        (true, true, false) => Some(B::Gt),
        (true, false, true) => Some(B::Lt),
        _ => None,
    }
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
            // Without a bounded guard, keep the original CASE and let
            // normal CASE simplification select its payload.
            let lower = a.as_ref().map(|n| b::ge(subject.clone(), b::lit_integer(n.clone())));
            let upper = z.as_ref().map(|n| b::le(subject.clone(), b::lit_integer(n.clone())));
            intervals.push(lower.into_iter().chain(upper).reduce(b::and)?);
        }
        if let Some(domain) = intervals.into_iter().reduce(b::or) {
            out.push((domain, value));
        }
    }
    Some((out, default))
}
