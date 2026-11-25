//! Finite exact decimals as signed integer coefficients and powers of ten.
//! Operations are distinct from fixed-width words and IEEE binary values.
use crate::{builder as b, Expr, Primitive};
use crate::domains::primitive::registry::FuncSemantics;
use num_bigint::{BigInt, Sign};
use std::cmp::Ordering;

pub const VALUE_TAG: &str = "exact_decimal";

const LITERAL: &str = "decimal.literal";
const PRODUCT: &str = "decimal.multiply";
const ROUND: &str = "decimal.round";

#[derive(Clone, Copy)]
pub enum Mode { HalfUp, HalfEven }

impl Mode {
    pub fn name(self) -> &'static str {
        match self { Self::HalfUp => "HALF_UP", Self::HalfEven => "HALF_EVEN" }
    }
}

pub enum View<'a> {
    Literal(BigInt, u32),
    Multiply(&'a Expr, &'a Expr),
    Round(&'a Expr, u32, Mode),
}

pub fn view(expr: &Expr) -> Option<View<'_>> {
    let (key, args) = expr.as_call()?;
    let natural = |expr: &Expr| expr.as_lit()?.as_integer()?.try_into().ok();
    match (crate::func_name(key).as_str(), args) {
        (LITERAL, [value, scale]) => Some(View::Literal(value.as_lit()?.as_integer()?, natural(scale)?)),
        (PRODUCT, [a, b]) => Some(View::Multiply(a, b)),
        (ROUND, [value, scale, mode]) => {
            let crate::Lit::Str(mode) = mode.as_lit()? else { return None };
            Some(View::Round(value, natural(scale)?, match mode.as_str() {
                "HALF_UP" => Mode::HalfUp,
                "HALF_EVEN" => Mode::HalfEven,
                _ => return None,
            }))
        }
        _ => None,
    }
}

pub fn is_decimal(expr: &Expr) -> bool {
    view(expr).is_some()
}

fn apply(name: &str, args: Vec<Expr>, types: Vec<Primitive>) -> Expr {
    let key = crate::register_func(name, types, Some(Primitive::Composite), FuncSemantics::pure_function());
    b::call(key, args)
}

pub fn literal(value: BigInt, scale: u32) -> Expr {
    apply(LITERAL, vec![b::lit_integer(value), b::lit_int(i64::from(scale))],
        vec![Primitive::Number, Primitive::Number])
}

pub fn multiply(a: Expr, b: Expr) -> Expr {
    if let (Some(View::Literal(x, s)), Some(View::Literal(y, t))) = (view(&a), view(&b)) {
        return literal(x*y, s.checked_add(t).expect("decimal scale overflow"));
    }
    apply(PRODUCT, vec![a, b], vec![Primitive::Composite, Primitive::Composite])
}

pub fn round(value: Expr, scale: u32, mode: Mode) -> Expr {
    if let Some(View::Literal(coefficient, digits)) = view(&value) {
        if scale >= digits {
            return literal(coefficient, digits);
        }
        let divisor = BigInt::from(10).pow(digits - scale);
        let negative = coefficient.sign() == Sign::Minus;
        let absolute = if negative { -coefficient } else { coefficient };
        let (mut quotient, remainder) = (&absolute / &divisor, &absolute % &divisor);
        let midpoint = (remainder * BigInt::from(2)).cmp(&divisor);
        if midpoint == Ordering::Greater || midpoint == Ordering::Equal
            && (matches!(mode, Mode::HalfUp) || &quotient % 2 == BigInt::from(1))
        {
            quotient += 1;
        }
        return literal(if negative { -quotient } else { quotient }, scale);
    }
    apply(ROUND, vec![value, b::lit_int(i64::from(scale)), b::lit_str(mode.name())],
        vec![Primitive::Composite, Primitive::Number, Primitive::String])
}

pub fn display_literal(value: BigInt, scale: u32) -> String {
    if scale == 0 { return value.to_string(); }
    let negative = value.sign() == Sign::Minus;
    let mut digits = value.magnitude().to_string();
    let width = scale as usize;
    if digits.len() <= width {
        digits.insert_str(0, &"0".repeat(width+1-digits.len()));
    }
    digits.insert(digits.len()-width, '.');
    while digits.ends_with('0') { digits.pop(); }
    if digits.ends_with('.') { digits.pop(); }
    if negative { digits.insert(0, '-'); }
    digits
}
