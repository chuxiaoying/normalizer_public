//! Closed immutable state values in the existing typed simulator and math IR.
use super::*;
use math::{domains::composite as data, Primitive};

impl Ty {
    pub(crate) fn describe(&self) -> String {
        match self {
            Self::Unit => "unit".into(),
            Self::Task(t) => format!("task<{}>", t.describe()),
            Self::Bool => "bool".into(),
            Self::Integer => "number".into(),
            Self::Decimal => "exact decimal".into(),
            Self::I32 => "i32".into(),
            Self::I64 => "i64".into(),
            Self::Text => "text?".into(),
            Self::OrderedTextMap => "map<text,text>".into(),
            Self::Ref => "ref?".into(),
            Self::Vector(t) => format!("vector<{}>", t.describe()),
            Self::Record(f) => format!(
                "record<{}>",
                f.iter()
                    .map(|(n, t)| format!("{n}:{}", t.describe()))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            _ => unreachable!("state types checked before description"),
        }
    }
    pub(super) fn state_value(&self) -> bool {
        match self {
            Self::Unit
            | Self::Bool
            | Self::Integer
            | Self::I32
            | Self::I64
            | Self::Text
            | Self::OrderedTextMap
            | Self::Ref => true,
            Self::Task(t) | Self::Vector(t) => t.state_value(),
            Self::Record(fields) => fields.values().all(Self::state_value),
            _ => false,
        }
    }
    // External immutable values carry no identity, callbacks, or heap mutation.
    // They are typed response data, not guessed library implementations.
    pub(crate) fn port_value(&self) -> bool {
        match self {
            Self::Unit | Self::Bool | Self::Integer | Self::I32 | Self::I64 | Self::Text => true,
            Self::Vector(t) => t.port_value(),
            Self::Record(fields) => fields.values().all(Self::port_value),
            _ => false,
        }
    }
    pub(crate) fn primitive_value(&self) -> Result<Primitive> {
        Ok(match self {
            Self::Unit => return Err(fail("unit has no primitive value")),
            Self::Bool => Primitive::Boolean,
            Self::Integer | Self::I32 | Self::I64 => Primitive::Number,
            Self::Text => Primitive::String,
            Self::Ref => Primitive::ReferenceIdentity,
            Self::Task(_)
            | Self::Vector(_)
            | Self::Record(_)
            | Self::OrderedTextMap => Primitive::Composite,
            _ => return Err(fail("state values cannot erase source sequence contracts")),
        })
    }
}
// Structural types are declarations, never external classes or executable code.
pub(super) fn structural_type(s: &str) -> Result<Option<Ty>> {
    fn parts(s: &str) -> Vec<&str> {
        let (mut depth, mut start) = (0, 0);
        let mut result = Vec::new();
        for (i, c) in s.char_indices() {
            match c {
                '<' => depth += 1,
                '>' => depth -= 1,
                ',' if depth == 0 => {
                    result.push(&s[start..i]);
                    start = i + 1;
                }
                _ => {}
            }
        }
        if !s.is_empty() {
            result.push(&s[start..]);
        }
        result
    }
    if let Some(t) = s.strip_prefix("task<").and_then(|s| s.strip_suffix('>')) {
        let t = Ty::parse(t)?;
        need(
            t.state_value() && !matches!(t, Ty::Task(_)),
            "task result must be a closed non-task value; adopt nested promises explicitly",
        )?;
        return Ok(Some(Ty::Task(Box::new(t))));
    }
    let vector_source = s.strip_suffix('?').unwrap_or(s);
    if let Some(t) = vector_source
        .strip_prefix("vector<")
        .and_then(|s| s.strip_suffix('>'))
    {
        let t = Ty::parse(t)?;
        need(t.state_value(), "vector element must be a state value")?;
        return Ok(Some(Ty::Vector(Box::new(t))));
    }
    if let Some(t) = s.strip_prefix("record<").and_then(|s| s.strip_suffix('>')) {
        let mut fields = BTreeMap::new();
        for p in parts(t) {
            let (name, typ) = p
                .split_once(':')
                .ok_or_else(|| fail("record field needs name:type"))?;
            let typ = Ty::parse(typ)?;
            need(
                identifier(name) && typ.state_value() && fields.insert(name.into(), typ).is_none(),
                "invalid/duplicate record field",
            )?;
        }
        return Ok(Some(Ty::Record(fields)));
    }
    Ok(None)
}
fn operation(op: &str, args: Vec<Value>, ty: Ty) -> Result<Value> {
    let params = args
        .iter()
        .map(|v| v.ty().primitive_value())
        .collect::<Result<Vec<_>>>()?;
    let args = args.iter().map(Value::scalar).collect::<Result<Vec<_>>>()?;
    let expr = data::apply(op, args, params, ty.primitive_value()?);
    Ok(Value::Term(ty, expr))
}
pub(super) fn field(record: Value, name: &str) -> Result<Value> {
    let Ty::Record(fields) = record.ty() else {
        return Err(fail("field requires immutable record"));
    };
    let ty = fields
        .get(name)
        .ok_or_else(|| fail("unknown record field"))?
        .clone();
    operation(
        "value.field",
        vec![record, Value::Term(Ty::Text, b::lit_str(name))],
        ty,
    )
}


pub(super) fn number_vector(items: Vec<Expr>) -> Result<Value> {
    let value = math::domains::composite::lifted_number_vector(items).map_err(fail)?;
    Ok(Value::Term(Ty::Vector(Box::new(Ty::Integer)), value))
}
