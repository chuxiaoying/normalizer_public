//! A full-rank conjunction of affine equalities on exact integer inputs has
//! at most one solution. Eliminate over the integers, then check divisibility
//! during substitution. Partial values and word arithmetic stay opaque.
use crate::{builder as b, BinOpKind, Expr, SymVarOrigin, UnaryOpKind};
use num_bigint::BigInt;
use num_traits::{One, Signed, Zero};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default, PartialEq, Eq)]
struct Equation {
    coefficients: BTreeMap<String, BigInt>,
    constant: BigInt,
}

impl Equation {
    fn accumulate(&mut self, value: &Expr, factor: &BigInt) -> Option<()> {
        if let Some(n) = value.as_lit().and_then(crate::Lit::as_integer) {
            self.constant += factor * n;
        } else if let Some(SymVarOrigin::Parameter { name, .. }) = value.as_sym_var() {
            if !super::integer_ac::total_integer(value) {
                return None;
            }
            *self.coefficients.entry(name.clone()).or_default() += factor;
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
    classes.into_iter().filter_map(|(equation, index, repeated)| {
        repeated.then(|| predicate(&equation).unwrap_or_else(|| conjuncts[index].clone()))
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
