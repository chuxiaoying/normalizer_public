//! Closed row-local expressions. A table binding, a view of it, and an
//! aggregate use the same query grammar; names bind in declaration order.
use super::{ScalarOperand, Tokens};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq)]
pub enum Scalar {
    Path(Vec<String>),
    Literal(ScalarOperand),
    Bool(bool),
    Binary(Op, Box<Scalar>, Box<Scalar>),
    NullPredicate { value: Box<Scalar>, negated: bool },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Op {
    Add,
    Sub,
    Mul,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Clause {
    Where(Scalar),
    Let(String, Scalar),
    Order(Vec<Scalar>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Projection {
    Value(Scalar),
    NestedSum(Box<Query>),
    GroupKeys(Vec<Scalar>),
    JoinCount { source: String, alias: String, on: Scalar, filters: Vec<Scalar> },
    FirstValue { value: Scalar, fallback: Scalar },
    Fields(Vec<(String, Scalar)>),
    Fold { accumulator: String, initial: Scalar, body: Scalar },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Rows,
    Sum,
    First,
    FirstScalar(usize),
    Reduce,
    Count,
    CountDistinct,
    GroupCount,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Query {
    pub binding: String,
    pub accumulate: bool,
    pub kind: Kind,
    pub source: String,
    pub alias: String,
    pub clauses: Vec<Clause>,
    pub projection: Projection,
}

fn atom(t: &mut Tokens<'_>) -> Result<Scalar, String> {
    if t.take("(") {
        let result = scalar(t)?;
        t.expect(")")?;
        return Ok(result);
    }
    match t.value()? {
        ScalarOperand::Local(n) if n == "true" || n == "false" => Ok(Scalar::Bool(n == "true")),
        ScalarOperand::Local(n) => {
            let mut path = vec![n];
            while t.take(".") {
                path.push(t.name()?);
            }
            Ok(Scalar::Path(path))
        }
        v => Ok(Scalar::Literal(v)),
    }
}

fn product(t: &mut Tokens<'_>) -> Result<Scalar, String> {
    let mut left = atom(t)?;
    while t.take("*") {
        left = Scalar::Binary(Op::Mul, Box::new(left), Box::new(atom(t)?));
    }
    Ok(left)
}

fn sum(t: &mut Tokens<'_>) -> Result<Scalar, String> {
    let mut left = product(t)?;
    loop {
        let op = if t.take("+") {
            Op::Add
        } else if t.take("-") {
            Op::Sub
        } else {
            break;
        };
        left = Scalar::Binary(op, Box::new(left), Box::new(product(t)?));
    }
    Ok(left)
}

fn scalar(t: &mut Tokens<'_>) -> Result<Scalar, String> {
    let left = sum(t)?;
    if t.take("IS") || t.take("is") {
        let negated = t.take("NOT") || t.take("not");
        if !t.take("NULL") { t.expect("null")?; }
        return Ok(Scalar::NullPredicate { value: Box::new(left), negated });
    }
    for (token, op) in [
        ("==", Op::Eq),
        ("!=", Op::Ne),
        ("<=", Op::Le),
        (">=", Op::Ge),
        (">", Op::Gt),
        ("<", Op::Lt),
    ] {
        if t.take(token) {
            return Ok(Scalar::Binary(op, Box::new(left), Box::new(sum(t)?)));
        }
    }
    Ok(left)
}

pub(super) fn parse(
    binding: String,
    accumulate: bool,
    t: &mut Tokens<'_>,
) -> Result<Option<Query>, String> {
    parse_query(binding, accumulate, t, true)
}

fn parse_query(
    binding: String,
    accumulate: bool,
    t: &mut Tokens<'_>,
    allow_nested_sum: bool,
) -> Result<Option<Query>, String> {
    if t.take("reduce") {
        if accumulate { return Err("reduce has its own initial value; cannot use +=".into()); }
        let source = t.name()?;
        t.expect("from")?;
        let initial = scalar(t)?;
        t.expect("as")?;
        t.expect("(")?;
        let accumulator = t.name()?;
        t.expect(",")?;
        let alias = t.name()?;
        t.expect(")")?;
        if accumulator == alias { return Err("reduce has duplicate binders".into()); }
        t.expect("yield")?;
        let body = scalar(t)?;
        if !t.rest().is_empty() { return Err("trailing reduce relation input".into()); }
        return Ok(Some(Query { binding, accumulate, kind: Kind::Reduce, source, alias, clauses: vec![],
            projection: Projection::Fold { accumulator, initial, body } }));
    }
    let kind = if t.take("sum") {
        Kind::Sum
    } else if t.take("first") {
        Kind::First
    } else if t.take("count") {
        if t.take("distinct") { Kind::CountDistinct }
        else if t.take("groups") { Kind::GroupCount } else { Kind::Count }
    } else {
        Kind::Rows
    };
    if !t.take("from") {
        return if kind == Kind::Rows {
            Ok(None)
        } else {
            Err("expected from after aggregate".into())
        };
    }
    if accumulate && kind != Kind::Sum {
        return Err("only sum supports +=".into());
    }
    let source = t.name()?;
    if kind == Kind::First {
        let offset = if t.take("[") {
            let ScalarOperand::Integer(offset) = t.value()? else {
                return Err("expected a nonnegative integer slice offset".into());
            };
            let offset = offset.to_string().parse::<usize>().map_err(|_| "invalid slice offset")?;
            if offset > i64::MAX as usize { return Err("slice offset outside integer range".into()); }
            t.expect(":")?;
            t.expect("]")?;
            Some(offset)
        } else {
            None
        };
        if offset.is_some() || t.rest().starts_with("else") {
            t.expect("else")?;
            let default = scalar(t)?;
            if !t.rest().is_empty() { return Err("trailing scalar first input".into()); }
            return Ok(Some(Query {
                binding, accumulate, kind: Kind::FirstScalar(offset.unwrap_or(0)), source,
                alias: String::new(), clauses: vec![], projection: Projection::Value(default),
            }));
        }
    }
    t.expect("as")?;
    let alias = t.name()?;
    if kind == Kind::GroupCount {
        t.expect("by")?;
        let mut keys = vec![scalar(t)?];
        while t.take(",") {
            keys.push(scalar(t)?);
        }
        if !t.rest().is_empty() { return Err("trailing group key clauses".into()); }
        return Ok(Some(Query { binding, accumulate, kind, source, alias,
            clauses: vec![], projection: Projection::GroupKeys(keys) }));
    }
    if kind == Kind::Count && t.take("join") {
        let right = t.name()?;
        t.expect("as")?;
        let right_alias = t.name()?;
        if right_alias == alias { return Err("join requires two distinct row aliases".into()); }
        t.expect("on")?;
        let on = scalar(t)?;
        let mut filters = vec![];
        if t.take("where") {
            filters.push(scalar(t)?);
            while t.take("and") { filters.push(scalar(t)?); }
        }
        if !t.rest().is_empty() { return Err("trailing join count input".into()); }
        return Ok(Some(Query {
            binding, accumulate, kind, source, alias, clauses: vec![],
            projection: Projection::JoinCount { source: right, alias: right_alias, on, filters },
        }));
    }
    let mut clauses = vec![];
    loop {
        if t.take("where") {
            clauses.push(Clause::Where(scalar(t)?));
            while t.take("and") {
                clauses.push(Clause::Where(scalar(t)?));
            }
        } else if t.take("let") {
            let name = t.name()?;
            t.expect("=")?;
            clauses.push(Clause::Let(name, scalar(t)?));
        } else if t.take("order") {
            t.expect("by")?;
            let mut keys = vec![scalar(t)?];
            while t.take(",") {
                keys.push(scalar(t)?);
            }
            clauses.push(Clause::Order(keys));
        } else {
            break;
        }
    }
    if matches!(kind, Kind::Count | Kind::CountDistinct) && t.rest().is_empty() {
        return Ok(Some(Query {
            binding, accumulate, kind, source, alias: alias.clone(), clauses,
            projection: Projection::Value(Scalar::Path(vec![alias])),
        }));
    }
    t.expect("yield")?;
    let has_nested_sum = {
        let mut lookahead = Tokens { source: t.rest() };
        lookahead.take("sum") && lookahead.take("from")
    };
    let mut projection = if t.take("{") {
        let mut fields = vec![];
        let mut names = BTreeSet::new();
        loop {
            let name = t.name()?;
            if !names.insert(name.clone()) {
                return Err("duplicate projected field".into());
            }
            t.expect(":")?;
            fields.push((name, scalar(t)?));
            if t.take("}") {
                break;
            }
            t.expect(",")?;
        }
        Projection::Fields(fields)
    } else if kind == Kind::Sum && has_nested_sum {
        if !allow_nested_sum {
            return Err("nested sum is limited to two dimensions".into());
        }
        let inner = parse_query(String::new(), false, t, false)?.ok_or("expected an inner sum")?;
        if inner.source != alias || inner.alias == alias || !matches!(inner.projection, Projection::Value(_)) {
            return Err("nested sum requires a scalar sum from the outer row with a distinct alias".into());
        }
        Projection::NestedSum(Box::new(inner))
    } else {
        Projection::Value(scalar(t)?)
    };
    if kind == Kind::First {
        t.expect("else")?;
        if !t.take("null") {
            let Projection::Value(value) = projection else {
                return Err("scalar first fallback requires a scalar projection".into());
            };
            projection = Projection::FirstValue { value, fallback: scalar(t)? };
        }
    }
    if !t.rest().is_empty() {
        return Err(format!("trailing relation input: {}", t.rest()));
    }
    Ok(Some(Query {
        binding,
        accumulate,
        kind,
        source,
        alias,
        clauses,
        projection,
    }))
}
