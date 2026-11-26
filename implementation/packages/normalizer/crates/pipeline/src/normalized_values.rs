//! Closed immutable state values in the existing typed simulator and math IR.
use super::*;
use math::{domains::composite as data, Primitive};

#[derive(Clone, Copy, PartialEq, Eq)]
enum ValueBoundary {
    ImmutablePort,
    State,
}

impl Ty {
    fn value_allowed(&self, boundary: ValueBoundary) -> bool {
        let state = boundary == ValueBoundary::State;
        match self {
            Self::Unit | Self::Bool | Self::Integer | Self::Text => true,
            Self::OrderedTextMap | Self::Ref => state,
            Self::Vector(t) => t.value_allowed(boundary),
            Self::Record(fields) => fields.values().all(|field| field.value_allowed(boundary)),
            Self::Task(_) | Self::Decimal | Self::LocalTextMap | Self::LocalRelation | Self::Seq(_) => false,
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
            Self::Vector(_)
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
        for p in t.split(',').filter(|_| !t.is_empty()) {
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
pub(super) fn field(record: Value, name: &str) -> Result<Value> {
    let Value::Term(Ty::Record(fields), value) = record else {
        return Err(fail("field requires immutable record"));
    };
    let ty = fields
        .get(name)
        .ok_or_else(|| fail("unknown record field"))?
        .clone();
    if ty == Ty::Integer {
        // Nonthrowing ports expose their declared record as the direct
        // completion root. Preserve the declared integer field in math's
        // typed receipt path rather than erase it into a generic Number call.
        // Throwing ports retain their partial completion.value projection.
        // The tagged completion is the only External symbol that carries a
        // checked Record. Scalar receipt paths cannot acquire that type.
        if let Some(math::SymVarOrigin::External { binding_id, .. }) = value.as_sym_var() {
            return Ok(Value::Term(
                ty,
                b::sym_e_path(*binding_id, vec![name.into()], ScalarSymbolType::Integer),
            ));
        }
    }
    let result_type = ty.primitive_value()?;
    let expr = data::apply(
        "value.field",
        vec![value, b::lit_str(name)],
        vec![Primitive::Composite, Primitive::String],
        result_type,
    );
    Ok(Value::Term(ty, expr))
}


pub(super) fn number_vector(items: Vec<Expr>) -> Result<Value> {
    let value = math::domains::composite::lifted_number_vector(items).map_err(fail)?;
    Ok(Value::Term(Ty::Vector(Box::new(Ty::Integer)), value))
}
