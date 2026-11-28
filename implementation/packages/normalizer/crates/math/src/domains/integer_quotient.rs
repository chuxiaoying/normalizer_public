//! BF-02: truncating integer quotient on the normal completion domain.
use crate::{builder as b, Expr, Lit, Primitive};
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
    if divisor.as_lit().and_then(Lit::as_integer).is_some_and(|value| value == 1.into()) {
        return dividend;
    }
    crate::normalize(b::call(key(), vec![dividend, divisor]))
}


// Reuse the defined quotient domain. This is a value identity over
// already observed operands; it does not execute either operand twice.
pub fn remainder_on_nonzero_path(dividend: Expr, divisor: Expr) -> Expr {
    if let (Some(a), Some(d)) = (dividend.as_lit().and_then(Lit::as_integer),
        divisor.as_lit().and_then(Lit::as_integer)) {
        return b::lit_integer(a % d);
    }
    let quotient = on_nonzero_path(dividend.clone(), divisor.clone());
    crate::normalize(b::sub(dividend, b::mul(divisor, quotient)))
}
