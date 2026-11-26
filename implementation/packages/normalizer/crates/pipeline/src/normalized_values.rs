//! Closed immutable state values in the existing typed simulator and math IR.
use super::*;
use math::{domains::composite as data, Primitive};

#[derive(Clone, Copy, PartialEq, Eq)]
enum ValueBoundary {
    ImmutablePort,
    State,
}

impl Ty {
    pub(crate) fn describe(&self) -> String {
        match self {
            Self::Unit => "unit".into(),
            Self::Task(t) => format!("task<{}>", t.describe()),
            Self::Bool => "bool".into(),
            Self::Integer => "number".into(),
            Self::Decimal => "exact decimal".into(),
            Self::Text => "text?".into(),
            Self::OrderedTextMap => "map<text,text>".into(),
            Self::LocalTextMap => "local nonescaping text map".into(),
            Self::LocalRelation => "local nonescaping relation".into(),
            Self::Ref => "ref?".into(),
            Self::Vector(t) => format!("vector<{}>", t.describe()),
            Self::Record(f) => format!(
                "record<{}>",
                f.iter()
                    .map(|(n, t)| format!("{n}:{}", t.describe()))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            Self::Seq(item) => format!("source sequence<{}>", item.describe()),
        }
    }
    fn value_allowed(&self, boundary: ValueBoundary) -> bool {
        let state = boundary == ValueBoundary::State;
        match self {
            Self::Unit | Self::Bool | Self::Integer | Self::Text => true,
            Self::OrderedTextMap | Self::Ref => state,
            Self::Task(t) => state && t.value_allowed(boundary),
            Self::Vector(t) => t.value_allowed(boundary),
            Self::Record(fields) => fields.values().all(|field| field.value_allowed(boundary)),
            Self::Decimal | Self::LocalTextMap | Self::LocalRelation | Self::Seq(_) => false,
        }
    }

    pub(super) fn state_value(&self) -> bool {
        self.value_allowed(ValueBoundary::State)
    }

    // External immutable values carry no identity, callbacks, or heap mutation.
    // They are typed response data, not guessed library implementations.
    pub(crate) fn port_value(&self) -> bool {
        self.value_allowed(ValueBoundary::ImmutablePort)
    }

    pub(crate) fn primitive_value(&self) -> Result<Primitive> {
        Ok(match self {
            Self::Bool => Primitive::Boolean,
            Self::Integer => Primitive::Number,
            Self::Text => Primitive::String,
            Self::Task(_)
            | Self::Vector(_)
            | Self::Record(_)
            | Self::OrderedTextMap => Primitive::Composite,
            _ => return Err(fail("state values cannot erase source sequence contracts")),
        })
    }
}
pub(super) fn valid_record_field(name: &str, typ: &Ty) -> bool {
    identifier(name) && typ.state_value()
}

// Structural types are declarations, never external classes or executable code.
pub(super) fn structural_type(s: &str) -> Result<Option<Ty>> {
    fn parts(s: &str) -> Vec<&str> {
        let (mut depth, mut start) = (0, 0);
        let mut result = Vec::new();
        for (i, c) in s.char_indices() {
            match c {
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
    if let Some(t) = s
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
                valid_record_field(name, &typ) && fields.insert(name.into(), typ).is_none(),
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
    if ty == Ty::Integer {
        let value = record.scalar()?;
        // Nonthrowing ports expose their declared record as the direct
        // completion root. Preserve the declared integer field in math's
        // typed receipt path rather than erase it into a generic Number call.
        // Throwing ports retain their partial completion.value projection.
        if let Some(math::SymVarOrigin::External { binding_id, path, primitive_type }) =
            value.as_sym_var()
        {
            if path.is_empty() && primitive_type == "normalized.completion" {
                return Ok(Value::Term(ty, b::sym_e_path(*binding_id, vec![name.into()], ScalarSymbolType::Integer)));
            }
        }
    }
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
