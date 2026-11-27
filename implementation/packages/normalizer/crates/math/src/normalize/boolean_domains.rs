//! CF-11 for complete CASE over closed, already acquired Boolean parameters.
//! Enumerate guards by cost on the actual remaining truth set, and use the
//! same structural key for guards and complete result trees.
use crate::{builder as b, BinOpKind as Op, Expr, SortContext, SymVarOrigin, UnaryOpKind};
use num_bigint::BigUint;
use num_traits::{One, Zero};
use std::collections::{BTreeMap, BTreeSet, HashMap};

type Truth = BigUint;

fn is_input(name: &str) -> bool {
    !name.starts_with('$') && !name.contains(['.', '[', ']'])
}

fn boolean(expr: &Expr, param: &mut impl FnMut(&str) -> Truth, all: &Truth) -> Option<Truth> {
    if let Some(value) = expr.as_bool() { return Some(if value { all.clone() } else { Truth::zero() }); }
    if let Some(SymVarOrigin::Parameter { name, primitive_type }) = expr.as_sym_var() {
        return (primitive_type == "boolean" && is_input(name)).then(|| param(name));
    }
    if let Some((UnaryOpKind::Not, operand)) = expr.as_unary_op() {
        return Some(all ^ boolean(operand, param, all)?);
    }
    let (op, left, right) = expr.as_binop()?;
    let (left, right) = (boolean(left, param, all)?, boolean(right, param, all)?);
    match op {
        Op::Eq => Some(all ^ (left ^ right)),
        Op::Ne => Some(left ^ right),
        Op::And => Some(left & right),
        Op::Or => Some(left | right),
        _ => None,
    }
}

pub(super) fn is_closed_boolean(expr: &Expr) -> bool {
    boolean(expr, &mut |_| Truth::zero(), &Truth::zero()).is_some()
}

fn closed_result(value: &Expr) -> bool {
    if let Some((arms, fallback)) = value.as_total_case_when() {
        // CF-11 owns a full Boolean partition, not only its outer arm.
        // A nested numeric or protected guard has its own domain proof.
        // Do not return an unchanged outer CASE and thereby block it.
        return arms.iter().all(|(guard, arm)| is_closed_boolean(guard) && closed_result(arm))
            && closed_result(fallback);
    }
    if super::boolean_casewhen::total_value(value) { return true; }
    if let Some(values) = value.as_array_lit() {
        return values.iter().all(closed_result);
    }
    if let Some(entries) = crate::domains::composite::ordered_text_map_entries(value) {
        return entries.iter().all(closed_result);
    }
    matches!(crate::domains::decimal::view(value), Some(crate::domains::decimal::View::Literal(..)))
}

struct Guards<'a> {
    all: Truth,
    sort: &'a dyn SortContext,
    known: BTreeMap<Truth, Expr>,
    layers: Vec<Vec<(Truth, Expr)>>,
    restricted: HashMap<(Truth, Truth), Expr>,
}

impl Guards<'_> {
    fn guard(&mut self, active: &Truth, desired: &Truth) -> Option<Expr> {
        if let Some(guard) = self.restricted.get(&(active.clone(), desired.clone())) { return Some(guard.clone()); }
        loop {
            if let Some(best) = self.known.iter()
                .filter(|(truth, _)| (*truth & active) == *desired)
                .map(|(_, guard)| guard)
                .min_by(|a, b| crate::canonical_order_compare(a, b, self.sort))
            {
                let best = best.clone();
                self.restricted.insert((active.clone(), desired.clone()), best.clone());
                return Some(best);
            }
            self.expand()?;
        }
    }

    fn expand(&mut self) -> Option<()> {
        let cost = self.layers.len();
        let mut added = BTreeMap::<Truth, Expr>::new();
        let mut propose = |truth: Truth, expr: Expr| -> Option<()> {
            if !super::require_candidate_budget(1 + (truth.bits() / 64) as usize, expr.size()) { return None; }
            if self.known.contains_key(&truth) { return Some(()); }
            let expr = crate::normalize_with(expr, self.sort);
            if added.get(&truth).is_none_or(|old| crate::canonical_order_compare(&expr, old, self.sort).is_lt()) {
                added.insert(truth, expr);
            }
            Some(())
        };
        for (truth, expr) in &self.layers[cost - 1] {
            propose(&self.all ^ truth, b::not(expr.clone()))?;
        }
        for left_cost in 0..=(cost - 1) / 2 {
            let right_cost = cost - 1 - left_cost;
            for (a, left) in &self.layers[left_cost] {
                for (z, right) in &self.layers[right_cost] {
                    let (left, right) = if crate::canonical_order_compare(left, right, self.sort).is_gt() {
                        (right, left)
                    } else { (left, right) };
                    propose(a & z, b::and(left.clone(), right.clone()))?;
                    propose(a | z, b::or(left.clone(), right.clone()))?;
                    propose(a ^ z, b::ne(left.clone(), right.clone()))?;
                    propose(&self.all ^ (a ^ z), b::eq(left.clone(), right.clone()))?;
                }
            }
        }
        self.known.extend(added.iter().map(|(mask, expr)| (mask.clone(), expr.clone())));
        self.layers.push(added.into_iter().collect());
        Some(())
    }
}

#[derive(Clone)]
struct Chain {
    arms: Vec<(Expr, Expr)>,
    fallback: Expr,
}

impl Chain {
    fn expression(&self) -> Expr {
        if self.arms.is_empty() { return self.fallback.clone(); }
        b::case_when(self.arms.clone(), self.fallback.clone())
    }
}

/// Each subset has one remaining domain. Extending its least suffix by a
/// fixed guard preserves both the additive cost order and the slot order.
/// This visits every eligible first arm without materializing permutations.
fn least_chain(
    subset: &Truth, groups: &[(Truth, Expr)], guards: &mut Guards<'_>, memo: &mut HashMap<Truth, Chain>,
) -> Option<Chain> {
    if subset.count_ones() == 1 {
        return Some(Chain { arms: vec![], fallback: groups[subset.trailing_zeros()? as usize].1.clone() });
    }
    if let Some(old) = memo.get(subset) { return Some(old.clone()); }
    let remaining = groups.iter().enumerate().filter(|(index, _)| subset.bit(*index as u64))
        .fold(Truth::zero(), |mask, (_, (truth, _))| mask | truth);
    let mut best: Option<(Expr, Chain)> = None;
    for (index, (truth, value)) in groups.iter().enumerate() {
        if !subset.bit(index as u64) { continue; }
        if !super::require_candidate_budget(1, groups.len()) { return None; }
        let guard = guards.guard(&remaining, truth)?;
        let mut rest = subset.clone();
        rest.set_bit(index as u64, false);
        let mut chain = least_chain(&rest, groups, guards, memo)?;
        chain.arms.insert(0, (guard, value.clone()));
        let expression = chain.expression();
        if best.as_ref().is_none_or(|(old, _)| crate::canonical_order_compare(&expression, old, guards.sort).is_lt()) {
            best = Some((expression, chain));
        }
    }
    let (_, best) = best?;
    memo.insert(subset.clone(), best.clone());
    Some(best)
}

pub(super) fn simplify(expr: &Expr, ctx: &dyn SortContext, parent: Option<&Expr>) -> Option<Expr> {
    fn inspectable(value: &Expr) -> Option<(&[(Expr, Expr)], Option<&Expr>)> {
        let (arms, fallback) = match value.as_total_case_when() {
            Some((arms, fallback)) => (arms, Some(fallback)),
            None => (value.as_case_when()?, None),
        };
        arms.iter().all(|(guard, _)| is_closed_boolean(guard)).then_some((arms, fallback))
    }
    let (arms, fallback) = inspectable(expr)?;
    if !arms.iter().all(|(_, value)| closed_result(value))
        || fallback.is_some_and(|value| !closed_result(value))
    { return None; }
    fn collect_names(value: &Expr, names: &mut BTreeSet<String>) {
        let Some((arms, fallback)) = inspectable(value) else { return; };
        for (guard, result) in arms {
            boolean(guard, &mut |name| { names.insert(name.to_owned()); Truth::zero() }, &Truth::zero())
                .expect("closed Boolean guard");
            collect_names(result, names);
        }
        if let Some(fallback) = fallback { collect_names(fallback, names); }
    }
    inspectable(expr)?;
    let mut names = BTreeSet::new();
    collect_names(expr, &mut names);
    if let Some(parent) = parent {
        boolean(parent, &mut |name| { names.insert(name.to_owned()); Truth::zero() }, &Truth::zero())?;
    }
    let Some(count) = 1usize.checked_shl(u32::try_from(names.len()).unwrap_or(u32::MAX)) else {
        super::stop_candidate_search(usize::MAX);
        return None;
    };
    // Charge the actual finite truth table before allocating or enumerating
    // it. This is a resource limit, not a fixed cap on admitted parameters.
    if !super::require_candidate_budget(count.saturating_mul(names.len().max(1)), count) { return None; }
    let all = (Truth::one() << count) - Truth::one();
    let vars: BTreeMap<_, _> = names.into_iter().enumerate().map(|(index, name)| {
        let mut truth = Truth::zero();
        for row in (0..count).filter(|row| row & (1 << index) != 0) {
            truth.set_bit(row as u64, true);
        }
        (name, truth)
    }).collect();
    let active = parent.map_or_else(|| Some(all.clone()), |parent| boolean(parent, &mut |name| vars[name].clone(), &all))?;
    if active.is_zero() {
        // The caller's path is unreachable. Do not publish false as a
        // replacement value for the unreachable expression.
        return Some(expr.clone());
    }
    let mut groups = Vec::<(Truth, Expr)>::new();
    fn collect_domains(
        value: &Expr, active: Truth, all: &Truth, vars: &BTreeMap<String, Truth>,
        groups: &mut Vec<(Truth, Expr)>,
    ) -> Option<()> {
        if active.is_zero() { return Some(()); }
        if let Some((arms, fallback)) = inspectable(value) {
            let mut remaining = active;
            for (guard, result) in arms {
                let truth = boolean(guard, &mut |name| vars[name].clone(), all)?;
                collect_domains(result, &remaining & &truth, all, vars, groups)?;
                remaining &= all ^ truth;
            }
            if let Some(fallback) = fallback {
                collect_domains(fallback, remaining, all, vars, groups)?;
            } else if !remaining.is_zero() {
                // A missing ELSE is admissible only if earlier first-match
                // domains exhaust the actual parent, never as a guessed value.
                return None;
            }
        } else if let Some((mask, _)) = groups.iter_mut().find(|(_, old)| old == value) {
            *mask |= active;
        } else {
            groups.push((active, value.clone()));
        }
        Some(())
    }
    // Native execution may build a tree of local CASEs. Derive each leaf's
    // actual result domain instead of taking one nested spelling as a leaf.
    collect_domains(expr, active.clone(), &all, &vars, &mut groups)?;
    if groups.len() == 1 { return Some(groups.pop()?.1); }
    let mut known = BTreeMap::from([(Truth::zero(), b::lit_bool(false)), (all.clone(), b::lit_bool(true))]);
    known.extend(vars.into_iter().map(|(name, truth)| (truth, b::param(name, crate::ScalarSymbolType::Boolean))));
    let layers = vec![known.iter().map(|(truth, value)| (truth.clone(), value.clone())).collect()];
    let mut guards = Guards { sort: ctx, all, known, layers, restricted: HashMap::new() };
    if groups.iter().all(|(_, value)| value.as_bool().is_some()) {
        let truth = groups.iter().filter(|(_, value)| value.as_bool() == Some(true))
            .fold(Truth::zero(), |mask, (truth, _)| mask | truth);
        return guards.guard(&active, &truth);
    }
    let subset = (Truth::one() << groups.len()) - Truth::one();
    least_chain(&subset, &groups, &mut guards, &mut HashMap::new()).map(|chain| chain.expression())
}
