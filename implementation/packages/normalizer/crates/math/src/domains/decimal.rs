//! Finite exact decimals as signed integer coefficients and powers of ten.
//! Operations are distinct from fixed-width words and IEEE binary values.
use crate::{builder as b, BinOpKind, Expr, Primitive};
use crate::domains::primitive::registry::FuncSemantics;
use num_bigint::{BigInt, Sign};
use std::cmp::Ordering;

pub const VALUE_TAG: &str = "exact_decimal";

const LITERAL: &str = "decimal.literal";
const PRODUCT: &str = "decimal.multiply";
const ROUND: &str = "decimal.round";

// This caps work for expansion, not the business value or its scale.
const MAX_RADIX_EXPANSION: u32 = 1_000_000;

fn check_expansion(digits: u32) {
    assert!(digits <= MAX_RADIX_EXPANSION,
        "[NORM-ANALYSIS-INCOMPLETE] exact decimal expansion exceeded the digit budget");
}

fn combined_scale(left: u32, right: u32) -> u32 {
    left.checked_add(right)
        .expect("[NORM-ANALYSIS-INCOMPLETE] exact decimal scale exceeds its representation")
}

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
    Round(&'a Expr, &'a Expr, Mode),
}

pub fn view(expr: &Expr) -> Option<View<'_>> {
    let (key, args) = expr.as_call()?;
    let natural = |expr: &Expr| expr.as_lit()?.as_integer()?.try_into().ok();
    match (crate::func_name(key).as_str(), args) {
        (LITERAL, [value, scale]) => Some(View::Literal(value.as_lit()?.as_integer()?, natural(scale)?)),
        (PRODUCT, [a, b]) => Some(View::Multiply(a, b)),
        (ROUND, [value, scale, mode]) => {
            let crate::Lit::Str(mode) = mode.as_lit()? else { return None };
            Some(View::Round(value, scale, match mode.as_str() {
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

pub fn literal(mut value: BigInt, mut scale: u32) -> Expr {
    // Canonicalize before a value enters CASE, equality or arithmetic. There
    // is one business number, not separate integer and decimal zero/one.
    if value.sign() == Sign::NoSign {
        return b::lit_int(0);
    }
    if scale != 0 {
        let trailing = value.to_str_radix(10).bytes().rev()
            .take(scale as usize).take_while(|&b| b == b'0').count() as u32;
        if trailing != 0 {
            value /= BigInt::from(10).pow(trailing);
            scale -= trailing;
        }
    }
    if scale == 0 {
        return b::lit_integer(value);
    }
    apply(LITERAL, vec![b::lit_integer(value), b::lit_int(i64::from(scale))],
        vec![Primitive::Number, Primitive::Number])
}

// Native Java spellings supply exact digits rather than a machine float.
pub fn from_radix_digits(digits: &str, radix: u32, power: i32) -> Option<Expr> {
    let coefficient = BigInt::parse_bytes(digits.as_bytes(), radix)?;
    let scale = power.unsigned_abs();
    check_expansion(scale);
    Some(if power >= 0 {
        let base = if radix == 16 { 2 } else { 10 };
        literal(coefficient * BigInt::from(base).pow(scale), 0)
    } else if radix == 16 {
        // 2^-n = 5^n / 10^n; no intermediate rounding.
        literal(coefficient * BigInt::from(5).pow(scale), scale)
    } else {
        literal(coefficient, scale)
    })
}

fn coefficient(expr: &Expr) -> Option<(BigInt, u32)> {
    if let Some(View::Literal(value, scale)) = view(expr) {
        Some((value, scale))
    } else {
        Some((expr.as_lit()?.as_integer()?, 0))
    }
}

pub fn negate(value: Expr) -> Expr {
    if let Some((value, scale)) = coefficient(&value) {
        literal(-value, scale)
    } else {
        b::neg(value)
    }
}

/// Operators admitted in the exact decimal domain. In particular, a generic
/// logical, bitwise or division operator cannot reach decimal folding.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Operator {
    Add, Sub, Mul, Eq, Ne, Lt, Le, Gt, Ge,
}

impl Operator {
    pub fn binop(self) -> BinOpKind {
        use Operator::*;
        match self {
            Add => BinOpKind::Add, Sub => BinOpKind::Sub, Mul => BinOpKind::Mul,
            Eq => BinOpKind::Eq, Ne => BinOpKind::Ne, Lt => BinOpKind::Lt,
            Le => BinOpKind::Le, Gt => BinOpKind::Gt, Ge => BinOpKind::Ge,
        }
    }
}

pub fn binary(op: Operator, left: Expr, right: Expr) -> Expr {
    use Operator::*;
    // The simulator evaluates both operands before reaching this pure rule;
    // neither a previously recorded effect nor an exceptional path is erased.
    if op == Sub && left == right {
        return b::lit_int(0);
    }
    if let (Some((a, s)), Some((b, t))) = (coefficient(&left), coefficient(&right)) {
        let scale = s.max(t);
        let align = || {
            check_expansion(scale - s.min(t));
            (&a * BigInt::from(10).pow(scale - s), &b * BigInt::from(10).pow(scale - t))
        };
        let compare = || {
            let (left, right) = align();
            left.cmp(&right)
        };
        return match op {
            Add => { let (a, b) = align(); literal(a + b, scale) }
            Sub => { let (a, b) = align(); literal(a - b, scale) }
            Mul => literal(&a * &b, combined_scale(s, t)),
            Eq => b::lit_bool(compare() == Ordering::Equal),
            Ne => b::lit_bool(compare() != Ordering::Equal),
            Lt => b::lit_bool(compare() == Ordering::Less),
            Le => b::lit_bool(compare() != Ordering::Greater),
            Gt => b::lit_bool(compare() == Ordering::Greater),
            Ge => b::lit_bool(compare() != Ordering::Less),
        };
    }
    b::binop_public(op.binop(), left, right)
}

pub fn multiply(a: Expr, b: Expr) -> Expr {
    if let (Some((x, s)), Some((y, t))) = (coefficient(&a), coefficient(&b)) {
        return literal(x*y, combined_scale(s, t));
    }
    apply(PRODUCT, vec![a, b], vec![Primitive::Composite, Primitive::Composite])
}

pub fn round(value: Expr, scale: u32, mode: Mode) -> Expr {
    round_at(value, b::lit_int(i64::from(scale)), mode)
}

/// Round a declared exact decimal with an already evaluated integer scale.
/// Negative scales round to tens, hundreds, and so on.
pub fn round_at(value: Expr, scale: Expr, mode: Mode) -> Expr {
    if let (Some((coefficient, digits)), Some(scale)) = (
        coefficient(&value), scale.as_lit().and_then(crate::Lit::as_integer),
    ) {
        let digits_value = BigInt::from(digits);
        if scale >= digits_value || coefficient.sign() == Sign::NoSign {
            return literal(coefficient, digits);
        }
        let removed = digits_value - &scale;
        let coefficient_digits = BigInt::from(coefficient.magnitude().to_str_radix(10).len());
        if removed > coefficient_digits {
            return b::lit_int(0);
        }
        let removed: u32 = removed.try_into()
            .expect("[NORM-ANALYSIS-INCOMPLETE] exact decimal rounding exceeds the digit budget");
        check_expansion(removed);
        let divisor = BigInt::from(10).pow(removed);
        let negative = coefficient.sign() == Sign::Minus;
        let absolute = if negative { -coefficient } else { coefficient };
        let (mut quotient, remainder) = (&absolute / &divisor, &absolute % &divisor);
        let midpoint = (remainder * BigInt::from(2)).cmp(&divisor);
        if midpoint == Ordering::Greater || midpoint == Ordering::Equal
            && (matches!(mode, Mode::HalfUp) || &quotient % 2 == BigInt::from(1))
        {
            quotient += 1;
        }
        if negative { quotient = -quotient; }
        if scale.sign() == Sign::Minus {
            let zeros = removed - digits;
            return b::lit_integer(quotient * BigInt::from(10).pow(zeros));
        }
        let scale: u32 = scale.try_into().expect("scale is nonnegative and smaller than the literal precision");
        return literal(quotient, scale);
    }
    apply(ROUND, vec![value, scale, b::lit_str(mode.name())],
        vec![Primitive::Composite, Primitive::Number, Primitive::String])
}

/// Render a canonical fractional atom returned by `view`.
/// Integral and zero values are ordinary integer expressions.
pub fn display_literal(value: BigInt, scale: u32) -> String {
    let negative = value.sign() == Sign::Minus;
    let mut digits = value.magnitude().to_string();
    let width = scale as usize;
    if digits.len() <= width {
        check_expansion(scale);
        digits.insert_str(0, &"0".repeat(width+1-digits.len()));
    }
    digits.insert(digits.len()-width, '.');
    if negative { digits.insert(0, '-'); }
    digits
}
