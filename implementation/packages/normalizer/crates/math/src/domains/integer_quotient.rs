//! BF-02: truncating integer quotient on the normal completion domain.
use crate::{builder as b, BinOpKind, Expr, Lit, Primitive};
use crate::domains::primitive::registry::{FuncKey, FuncSemantics};
use num_bigint::Sign;
use std::sync::{Arc, OnceLock};

pub const OUTPUT_NAME: &str = "QUOTIENT";

fn key() -> FuncKey {
    static KEY: OnceLock<FuncKey> = OnceLock::new();
    *KEY.get_or_init(|| crate::register_func(
        "integer_quotient",
        vec![Primitive::Number, Primitive::Number],
        Some(Primitive::Number),
        FuncSemantics::pure_function().with_evaluator(Arc::new(|args| {
            let [dividend, divisor] = args else { return None; };
            let divisor = divisor.as_integer()?;
            if divisor.sign() == Sign::NoSign { return None; }
            let quotient = dividend.as_integer()? / divisor;
            b::lit_integer(quotient).as_lit().cloned()
        })),
    ))
}

pub fn is_quotient(expr: &Expr) -> bool {
    expr.as_call().is_some_and(|(candidate, args)| args.len() == 2 && candidate == key())
}

/// Both operands are already evaluated integers. The language executor
/// separates division by zero before constructing this value.
pub fn on_nonzero_path(dividend: Expr, divisor: Expr) -> Expr {
    if is_exact_zero(&dividend) {
        return dividend;
    }
    if divisor.as_lit().and_then(Lit::as_integer).is_some_and(|value| value == 1.into()) {
        return dividend;
    }
    if let Some(over) = cancel_exact_factor(&dividend, &divisor) {
        return crate::normalize(over);
    }
    quotient_call(dividend, divisor)
}

fn quotient_call(dividend: Expr, divisor: Expr) -> Expr {
    crate::normalize(b::call(key(), vec![dividend, divisor]))
}

fn is_exact_zero(expr: &Expr) -> bool {
    expr.as_lit().and_then(Lit::as_integer).is_some_and(|n| n.sign() == Sign::NoSign)
}

// Reuse the defined quotient domain. This is a value identity over
// already observed operands; it does not execute either operand twice.
pub fn remainder_on_nonzero_path(dividend: Expr, divisor: Expr) -> Expr {
    if let (Some(a), Some(d)) = (dividend.as_lit().and_then(Lit::as_integer),
        divisor.as_lit().and_then(Lit::as_integer)) {
        return b::lit_integer(a % d);
    }
    if is_exact_zero(&dividend)
        || divisor.as_lit().and_then(Lit::as_integer).is_some_and(|d| d == 1.into() || d == (-1).into()) {
        return b::lit_int(0);
    }
    if cancel_exact_factor(&dividend, &divisor).is_some() {
        return b::lit_int(0);
    }
    let quotient = quotient_call(dividend.clone(), divisor.clone());
    crate::normalize(b::sub(dividend, b::mul(divisor, quotient)))
}

// Match an already acquired factor, never an algebraically similar
// expression or a word multiplication with its own wrap contract.
fn cancel_exact_factor(product: &Expr, divisor: &Expr) -> Option<Expr> {
    let (BinOpKind::Mul, left, right) = product.as_binop()? else { return None; };
    if left == divisor { return Some(right.clone()); }
    if right == divisor { return Some(left.clone()); }
    cancel_exact_factor(left, divisor).map(|rest| b::mul(rest, right.clone()))
        .or_else(|| cancel_exact_factor(right, divisor).map(|rest| b::mul(left.clone(), rest)))
}
