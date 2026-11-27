//! Exact decimal source syntax, without inference from IEEE binary carriers.
use super::{ScalarOperand, Tokens};
use num_bigint::BigInt;

#[derive(Clone, Debug, PartialEq)]
pub struct Assignment {
    pub binding: String,
    pub value: Decimal,
    pub round: Option<(u32, Mode)>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    HalfUp,
    HalfEven,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Decimal {
    Local(String),
    Literal { coefficient: BigInt, scale: u32 },
    Product(Vec<Decimal>),
}

fn decimal(t: &mut Tokens<'_>) -> Result<Decimal, String> {
    let input = t.rest();
    if !input.starts_with(|c: char| c == '-' || c.is_ascii_digit()) {
        return Ok(Decimal::Local(t.name()?));
    }
    let sign = usize::from(input.starts_with('-'));
    let whole_end = sign + input[sign..].bytes().take_while(u8::is_ascii_digit).count();
    if sign == whole_end { return Err("decimal requires digits".into()); }
    let fraction = input[whole_end..].strip_prefix('.').map(|tail|
        tail.bytes().take_while(u8::is_ascii_digit).count());
    if fraction == Some(0) { return Err("decimal requires digits after the point".into()); }
    let scale = fraction.unwrap_or(0);
    let end = whole_end + fraction.map_or(0, |n| n+1);
    let scale = u32::try_from(scale).map_err(|_| "[NORM-ANALYSIS-INCOMPLETE] exact decimal scale exceeds its representation")?;
    let digits = input[..end].replace('.', "");
    let coefficient = digits.parse().map_err(|_| "invalid decimal literal")?;
    t.source = &input[end..];
    Ok(Decimal::Literal { coefficient, scale })
}

pub(super) fn parse(binding: &str, t: &mut Tokens<'_>) -> Result<Option<Assignment>, String> {
    let mut prefix = Tokens { source: t.source };
    if prefix.take("round") && prefix.take("(") {
        *t = prefix;
        let mut factors = vec![decimal(t)?];
        while t.take("*") { factors.push(decimal(t)?); }
        let value = if factors.len() == 1 { factors.pop().unwrap() } else { Decimal::Product(factors) };
        t.expect(",")?;
        let ScalarOperand::Integer(scale) = t.value()? else { return Err("round scale must be an integer literal".into()); };
        if scale.sign() == num_bigint::Sign::Minus { return Err("round scale must be nonnegative".into()); }
        let scale: u32 = scale.try_into()
            .map_err(|_| "[NORM-ANALYSIS-INCOMPLETE] round scale exceeds its representation")?;
        if !t.take(",") { return Err("decimal rounding requires an explicit midpoint mode".into()); }
        let mode = match t.name()?.as_str() {
            "HALF_UP" => Mode::HalfUp,
            "HALF_EVEN" => Mode::HalfEven,
            _ => return Err("unknown decimal rounding mode".into()),
        };
        t.expect(")")?;
        t.end("trailing decimal round input")?;
        return Ok(Some(Assignment { binding: binding.into(), value, round: Some((scale, mode)) }));
    }
    let mut value = Tokens { source: t.source };
    let parsed = match decimal(&mut value) {
        Ok(parsed) => parsed,
        Err(error) if error.starts_with("[NORM-ANALYSIS-INCOMPLETE]") => return Err(error),
        Err(_) => return Ok(None),
    };
    if !value.rest().is_empty() { return Ok(None); }
    *t = value;
    Ok(Some(Assignment { binding: binding.into(), value: parsed, round: None }))
}
