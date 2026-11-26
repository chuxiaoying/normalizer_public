//! Exact decimal source syntax, without inference from IEEE binary carriers.
use super::{ScalarOperand, Tokens};
use num_bigint::BigInt;

#[derive(Clone, Debug, PartialEq)]
pub struct Assignment {
    pub binding: String,
    pub value: Decimal,
    pub round: Option<(u32, Option<Mode>)>,
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
    if end > 4096 { return Err("decimal literal exceeds 4096 characters".into()); }
    let digits = input[..end].replace('.', "");
    let coefficient = digits.parse().map_err(|_| "invalid decimal literal")?;
    t.source = &input[end..];
    Ok(Decimal::Literal { coefficient, scale: scale as u32 })
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
        let scale: u32 = scale.try_into().map_err(|_| "round scale must be nonnegative")?;
        if scale > 4096 { return Err("round scale exceeds 4096".into()); }
        let mode = if t.take(",") {
            Some(match t.name()?.as_str() {
                "HALF_UP" => Mode::HalfUp,
                "HALF_EVEN" => Mode::HalfEven,
                _ => return Err("unknown decimal rounding mode".into()),
            })
        } else { None };
        t.expect(")")?;
        t.end("trailing decimal round input")?;
        return Ok(Some(Assignment { binding: binding.into(), value, round: Some((scale, mode)) }));
    }
    let Ok(name) = t.name() else { return Ok(None) };
    if !t.rest().is_empty() { return Ok(None); }
    Ok(Some(Assignment { binding: binding.into(), value: Decimal::Local(name), round: None }))
}
