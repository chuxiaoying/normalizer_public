//! Closed syntax for a local ordered stack. A check precedes each pop.
use super::Tokens;

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    PushAll { stack: String, source: String },
    CheckAndPop { stack: String, source: String, field: String, exception: String },
    IsEmpty { binding: String, stack: String },
}

pub(super) fn parse(binding: &str, tokens: &mut Tokens<'_>) -> Result<Option<Action>, String> {
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
        tokens.end("unexpected local stack clauses")?;
        return Ok(Some(Action::IsEmpty { binding: binding.into(), stack }));
    }
    if !tokens.take("fold") || tokens.rest().starts_with('[') { return Ok(None); }
    let source = tokens.name()?;
    tokens.expect("from")?;
    let initialize = tokens.take("[");
    if initialize { tokens.expect("]")?; }
    else { tokens.bound(binding, "unbound local stack operand")?; }
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
        tokens.bound(&accumulator, "unbound local stack operand")?;
        tokens.expect(",")?;
        tokens.bound(&item, "unbound local stack operand")?;
        tokens.expect(")")?;
        tokens.end("unexpected local stack clauses")?;
        return Ok(Some(Action::PushAll { stack: binding.into(), source }));
    }
    tokens.expect("when")?;
    op(tokens, "length")?;
    tokens.bound(&accumulator, "unbound local stack operand")?;
    tokens.expect(")")?;
    tokens.expect("==")?;
    tokens.expect("0")?;
    tokens.expect("throw")?;
    let exception = tokens.name()?;
    tokens.expect("when")?;
    op(tokens, "last_or")?;
    tokens.bound(&accumulator, "unbound local stack operand")?;
    tokens.expect(",")?;
    tokens.bound(&item, "unbound local stack operand")?;
    tokens.expect(")")?;
    tokens.expect(".")?;
    let field = tokens.name()?;
    tokens.expect("!=")?;
    tokens.bound(&item, "unbound local stack operand")?;
    tokens.expect(".")?;
    tokens.bound(&field, "unbound local stack operand")?;
    tokens.expect("throw")?;
    tokens.bound(&exception, "unbound local stack operand")?;
    tokens.expect("yield")?;
    op(tokens, "pop")?;
    tokens.bound(&accumulator, "unbound local stack operand")?;
    tokens.expect(")")?;
    tokens.end("unexpected local stack clauses")?;
    Ok(Some(Action::CheckAndPop { stack: binding.into(), source, field, exception }))
}
