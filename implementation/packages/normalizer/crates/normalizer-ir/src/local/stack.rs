//! Closed syntax for a local ordered stack. A check precedes each pop.
use super::Tokens;

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    PushAll { stack: String, source: String },
    CheckAndPop { stack: String, source: String, field: String, exception: String },
    IsEmpty { binding: String, stack: String },
}

pub(super) fn parse(binding: &str, tokens: &mut Tokens<'_>) -> Result<Option<Action>, String> {
    let same = |t: &mut Tokens<'_>, expected: &str| -> Result<(), String> {
        if t.name()? == expected { Ok(()) } else { Err("unbound local stack operand".into()) }
    };
    let finish = |t: &Tokens<'_>| {
        if t.rest().is_empty() { Ok(()) } else { Err("unexpected local stack clauses".to_owned()) }
    };
    let op = |t: &mut Tokens<'_>, name: &str| -> Result<(), String> {
        t.expect("value")?;
        t.expect(".")?;
        t.expect(name)?;
        t.expect("(")
    };
    if let Some(rest) = tokens.rest().strip_prefix("value.length(") {
        tokens.source = rest;
        let stack = tokens.name()?;
        tokens.expect(")")?;
        tokens.expect("==")?;
        tokens.expect("0")?;
        finish(tokens)?;
        return Ok(Some(Action::IsEmpty { binding: binding.into(), stack }));
    }
    if !tokens.take("fold") || tokens.rest().starts_with('[') { return Ok(None); }
    let source = tokens.name()?;
    tokens.expect("from")?;
    let initialize = tokens.take("[");
    if initialize { tokens.expect("]")?; }
    else { same(tokens, binding)?; }
    tokens.expect("as")?;
    tokens.expect("(")?;
    let accumulator = tokens.name()?;
    tokens.expect(",")?;
    let item = tokens.name()?;
    tokens.expect(")")?;
    if [binding, source.as_str(), accumulator.as_str(), item.as_str()].into_iter()
        .collect::<std::collections::BTreeSet<_>>().len() != 4
    {
        return Err("stack, input and fold aliases must be distinct".into());
    }
    if initialize {
        tokens.expect("yield")?;
        op(tokens, "push")?;
        same(tokens, &accumulator)?;
        tokens.expect(",")?;
        same(tokens, &item)?;
        tokens.expect(")")?;
        finish(tokens)?;
        return Ok(Some(Action::PushAll { stack: binding.into(), source }));
    }
    tokens.expect("when")?;
    op(tokens, "length")?;
    same(tokens, &accumulator)?;
    tokens.expect(")")?;
    tokens.expect("==")?;
    tokens.expect("0")?;
    tokens.expect("throw")?;
    let exception = tokens.name()?;
    tokens.expect("when")?;
    op(tokens, "last_or")?;
    same(tokens, &accumulator)?;
    tokens.expect(",")?;
    same(tokens, &item)?;
    tokens.expect(")")?;
    tokens.expect(".")?;
    let field = tokens.name()?;
    tokens.expect("!=")?;
    same(tokens, &item)?;
    tokens.expect(".")?;
    same(tokens, &field)?;
    tokens.expect("throw")?;
    same(tokens, &exception)?;
    tokens.expect("yield")?;
    op(tokens, "pop")?;
    same(tokens, &accumulator)?;
    tokens.expect(")")?;
    finish(tokens)?;
    Ok(Some(Action::CheckAndPop { stack: binding.into(), source, field, exception }))
}
