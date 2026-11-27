//! A full-rank conjunction of affine equalities on exact integer inputs has
//! at most one solution. Eliminate over the integers, then check divisibility
//! during substitution. Partial values and word arithmetic stay opaque.
use crate::{builder as b, BinOpKind, Expr, SortContext, SymVarOrigin};
use num_bigint::BigInt;
use num_traits::{One, Signed, Zero};
use std::collections::{BTreeMap, BTreeSet};

/// Move exact literal offsets in a two-sided affine equality to the
/// canonical *other* operand. Subject identities and the relation itself do
/// not change; this is not a solver for an underdetermined equation.
pub(super) fn isolate_offset(expr: &Expr, ctx: &dyn SortContext) -> Option<Expr> {
    let (op @ (BinOpKind::Eq | BinOpKind::Ne), left, right) = expr.as_binop()? else {
        return None;
    };
    // An opaque receipt or staged observation does not become a total
    // integer merely because the comparison returns a Boolean.
    if [left, right].iter().any(|value| value.size() > 256 || value.depth() > 48
        || !super::integer_ac::total_integer(value))
    {
        return None;
    }
    let (mut left, mut left_offset) = split_offset(left);
    let (mut right, mut right_offset) = split_offset(right);
    if !affine_subject(left) || !affine_subject(right) {
        return None;
    }
    // Two row values may belong to different scopes. This rule admits a
    // single acquired row against one exact entry input, not comparisons
    // between rows; preserve the original scoped operand.
    if integer_row(left).is_some() && integer_row(right).is_some() {
        return None;
    }
    if left_offset.is_zero() && right_offset.is_zero() { return None; }
    if left == right {
        return Some(b::lit_bool((left_offset == right_offset) == (op == BinOpKind::Eq)));
    }
    if crate::canonical_order_compare(left, right, ctx).is_gt() {
        std::mem::swap(&mut left, &mut right);
        std::mem::swap(&mut left_offset, &mut right_offset);
    }

    let offset = right_offset - left_offset;
    let adjusted = if offset.is_zero() {
        right.clone()
    } else if offset.is_positive() {
        b::add(right.clone(), b::lit_integer(offset))
    } else {
        b::sub(right.clone(), b::lit_integer(-offset))
    };
    let result = b::binop_public(op, left.clone(), adjusted);
    (result != *expr).then_some(result)
}

fn integer_row(expr: &Expr) -> Option<&str> {
    match expr.kind() {
        crate::expr::ExprKind::RowValue { alias, acquired_integer: true, .. } => Some(alias),
        _ => None,
    }
}

fn affine_subject(expr: &Expr) -> bool {
    direct_integer_input(expr).is_some() || integer_row(expr).is_some()
}

fn direct_integer_input(expr: &Expr) -> Option<&str> {
    match expr.as_sym_var()? {
        SymVarOrigin::Parameter { name, primitive_type }
            if primitive_type == "integer"
                && !name.starts_with('$') && !name.contains(['.', '[', ']']) => Some(name),
        _ => None,
    }
}

fn split_offset(mut value: &Expr) -> (&Expr, BigInt) {
    let mut offset = BigInt::zero();
    while let Some((op, left, right)) = value.as_binop() {
        match op {
            BinOpKind::Add => {
                if let Some(n) = right.as_lit().and_then(crate::Lit::as_integer) {
                    offset += n;
                    value = left;
                } else if let Some(n) = left.as_lit().and_then(crate::Lit::as_integer) {
                    offset += n;
                    value = right;
                } else {
                    break;
                }
            }
            BinOpKind::Sub => {
                let Some(n) = right.as_lit().and_then(crate::Lit::as_integer) else { break };
                offset -= n;
                value = left;
            }
            _ => break,
        }
    }
    (value, offset)
}

#[derive(Default, PartialEq, Eq)]
struct Equation {
    coefficients: BTreeMap<String, BigInt>,
    constant: BigInt,
}

impl Equation {
    fn accumulate(&mut self, value: &Expr, factor: &BigInt) -> Option<()> {
        if let Some(n) = value.as_lit().and_then(crate::Lit::as_integer) {
            self.constant += factor * n;
        } else if let Some(name) = direct_integer_input(value) {
            *self.coefficients.entry(name.to_owned()).or_default() += factor;
        } else {
            let (op, left, right) = value.as_binop()?;
            match op {
                BinOpKind::Add | BinOpKind::Sub => {
                    self.accumulate(left, factor)?;
                    self.accumulate(
                        right,
                        &if op == BinOpKind::Sub {
                            -factor
                        } else {
                            factor.clone()
                        },
                    )?;
                }
                BinOpKind::Mul => {
                    let (multiple, term) = left
                        .as_lit()
                        .and_then(crate::Lit::as_integer)
                        .map(|n| (n, right))
                        .or_else(|| {
                            right
                                .as_lit()
                                .and_then(crate::Lit::as_integer)
                                .map(|n| (n, left))
                        })?;
                    self.accumulate(term, &(factor * multiple))?;
                }
                _ => return None,
            }
        }
        Some(())
    }
}

/// MA-02: isolate a single acquired integer in an affine comparison.
pub(super) fn isolate_single_subject(expr: &Expr) -> Option<Expr> {
    let (op, left, right) = expr.as_binop()?;
    let mut op = op.comparison()?;
    let affine_work = |operand: &Expr| matches!(operand.as_binop(),
        Some((BinOpKind::Add | BinOpKind::Sub | BinOpKind::Mul, _, _)));
    if !affine_work(left) && !affine_work(right) { return None; }
    let nodes = expr.size();
    if nodes > 256 || expr.depth() > 48 || !super::consume_proof_budget(nodes, nodes) {
        return None;
    }
    let mut affine = Equation::default();
    affine.accumulate(left, &BigInt::one())?;
    affine.accumulate(right, &-BigInt::one())?;
    affine.coefficients.retain(|_, coefficient| !coefficient.is_zero());
    use crate::expr::Comparison::*;
    if affine.coefficients.is_empty() {
        return Some(b::lit_bool(BinOpKind::from(op).comparison_test::<BigInt>()?(&affine.constant, &BigInt::zero())));
    }
    if affine.coefficients.len() != 1 { return None; }
    let (name, mut coefficient) = affine.coefficients.into_iter().next()?;
    let mut bound = -affine.constant;
    if coefficient.is_negative() {
        coefficient = -coefficient;
        bound = -bound;
        op = op.swapped();
    }
    let quotient = &bound / &coefficient;
    let exact = (&bound % &coefficient).is_zero();
    let floor = if !exact && bound.is_negative() { &quotient - 1 } else { quotient };
    let ceil = if exact { floor.clone() } else { &floor + 1 };
    let (direction, endpoint) = match op {
        Eq | Ne if !exact => return Some(b::lit_bool(matches!(op, Ne))),
        Eq | Ne => (op, floor),
        Lt => (Lt, ceil),
        Le => (Lt, floor + 1),
        Gt => (Gt, floor),
        Ge => (Gt, ceil - 1),
    };
    let result = b::binop_public(direction.into(),
        b::param(name, crate::ScalarSymbolType::Integer), b::lit_integer(endpoint));
    (result != *expr).then_some(result)
}

// Divide both sides by a nonzero common factor and fix the leading
// sign. This identifies equal integer solution sets without selecting
// particular values for free inputs.
fn primitive(mut equation: Equation) -> Equation {
    let mut factor = equation.constant.abs();
    for coefficient in equation.coefficients.values() {
        let mut term = coefficient.abs();
        while !term.is_zero() {
            let remainder = &factor % &term;
            factor = term;
            term = remainder;
        }
    }
    if !factor.is_zero() && !factor.is_one() {
        equation.constant /= &factor;
        for coefficient in equation.coefficients.values_mut() {
            *coefficient /= &factor;
        }
    }
    if equation.coefficients.values().next().unwrap_or(&equation.constant).is_negative() {
        equation.constant = -equation.constant;
        for coefficient in equation.coefficients.values_mut() {
            *coefficient = -std::mem::take(coefficient);
        }
    }
    equation
}

fn predicate(equation: &Equation) -> Option<Expr> {
    let left = equation.coefficients.iter().map(|(name, coefficient)| {
        let input = b::param(name, crate::ScalarSymbolType::Integer);
        if coefficient.is_one() { input } else { b::mul(b::lit_integer(coefficient.clone()), input) }
    }).reduce(b::add)?;
    Some(b::eq(left, b::lit_integer(-&equation.constant)))
}

fn remove_repeated_classes(equations: Vec<Equation>, conjuncts: &[Expr]) -> Option<Expr> {
    let mut classes: Vec<(Equation, usize, bool)> = Vec::new();
    for (index, equation) in equations.into_iter().enumerate() {
        let equation = primitive(equation);
        if let Some((_, _, repeated)) = classes.iter_mut().find(|(e, _, _)| *e == equation) {
            *repeated = true;
        } else {
            classes.push((equation, index, false));
        }
    }
    if classes.len() == conjuncts.len() { return None; }
    classes.into_iter().map(|(equation, index, repeated)| {
        if repeated {
            predicate(&equation).unwrap_or_else(|| conjuncts[index].clone())
        } else {
            conjuncts[index].clone()
        }
    }).reduce(b::and)
}

pub(super) fn solve(conjuncts: &[Expr]) -> Option<Expr> {
    // Keep online elimination bounded. Larger or underdetermined systems
    // retain their exact predicates; the budget never changes their meaning.
    // The caller flattens a binary AND, providing at least two conjuncts.
    let mut equations = Vec::new();
    let mut variables = BTreeSet::new();
    for predicate in conjuncts {
        let (BinOpKind::Eq, left, right) = predicate.as_binop()? else {
            return None;
        };
        let mut equation = Equation::default();
        equation.accumulate(left, &BigInt::one())?;
        equation.accumulate(right, &-BigInt::one())?;
        equation.coefficients.retain(|_, value| !value.is_zero());
        variables.extend(equation.coefficients.keys().cloned());
        equations.push(equation);
    }
    solve_full_rank(&equations, variables).or_else(|| remove_repeated_classes(equations, conjuncts))
}

fn solve_full_rank(equations: &[Equation], variables: BTreeSet<String>) -> Option<Expr> {
    let n = variables.len();
    if n > equations.len() || equations.iter().all(|eq| eq.coefficients.len() < 2) {
        return None;
    }
    let variables: Vec<_> = variables.into_iter().collect();
    let mut rows: Vec<Vec<BigInt>> = equations
        .into_iter()
        .map(|eq| {
            variables
                .iter()
                .map(|v| eq.coefficients.get(v).cloned().unwrap_or_default())
                .chain([-&eq.constant])
                .collect()
        })
        .collect();

    // Fraction-free Gaussian elimination (Bareiss). Every intermediate
    // division is exact, including when the matrix needs a row exchange.
    let mut previous = BigInt::one();
    for col in 0..n {
        let pivot = (col..rows.len()).find(|&r| !rows[r][col].is_zero())?;
        rows.swap(col, pivot);
        let (leading, tail) = rows.split_at_mut(col + 1);
        let pivot = &leading[col];
        for row in tail {
            for c in col + 1..=n {
                row[c] = (&pivot[col] * &row[c] - &row[col] * &pivot[c]) / &previous;
            }
            row[col] = BigInt::zero();
        }
        previous = pivot[col].clone();
    }
    // The n pivot columns are zero in every remaining row. Any nonzero
    // right-hand side in that tail is an inconsistent original constraint.
    if rows[n..].iter().any(|row| !row[n].is_zero()) {
        return Some(b::lit_bool(false));
    }
    let mut solution = vec![BigInt::zero(); n];
    for col in (0..n).rev() {
        let row = &rows[col];
        let remainder = &row[n] - (col + 1..n).map(|c| &row[c] * &solution[c]).sum::<BigInt>();
        if (&remainder % &row[col]) != BigInt::zero() {
            return Some(b::lit_bool(false));
        }
        solution[col] = remainder / &row[col];
    }
    variables
        .iter()
        .zip(solution)
        .map(|(name, value)| b::eq(b::param(name, crate::ScalarSymbolType::Integer), b::lit_integer(value)))
        .reduce(b::and)
}
