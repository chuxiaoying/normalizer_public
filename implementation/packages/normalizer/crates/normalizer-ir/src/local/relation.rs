//! Closed row-local expressions. A table binding, a view of it, and an
//! aggregate use the same query grammar; names bind in declaration order.
use super::{ScalarOperand, Tokens};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq)]
pub enum Scalar {
    Path(Vec<String>),
    Indexed {
        source: String,
        index: Box<Scalar>,
        fields: Vec<String>,
    },
    Integer(num_bigint::BigInt),
    Text(String),
    Bool(bool),
    IntegerBound(IntegerBound, Vec<Scalar>),
    Binary(Op, Box<Scalar>, Box<Scalar>),
    NullPredicate {
        owner: String,
        column: String,
        negated: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum IntegerBound {
    Min,
    Max,
    Length,
}

#[derive(Clone, Debug, PartialEq)]
pub struct IntegerRange {
    pub binding: String,
    pub accumulate: bool,
    pub lower: Scalar,
    pub upper: Scalar,
    pub select: RangeOperation,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RangeOperation {
    Count,
    Sum,
    FilteredCount(RangeScope),
    ProjectedSum {
        scope: RangeScope,
        value: Scalar,
    },
    First {
        scope: RangeScope,
        value: Scalar,
        fallback: Scalar,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct RangeScope {
    pub alias: String,
    pub filters: Vec<Scalar>,
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
pub enum Operation {
    FirstArray {
        offset: num_bigint::BigUint,
        fallback: Scalar,
    },
    NestedSum {
        alias: String,
        filters: Vec<Scalar>,
        value: Scalar,
    },
    JoinCount {
        source: String,
        alias: String,
        on: Scalar,
        filters: Vec<Scalar>,
    },
    OnRows(RowOperation),
}

#[derive(Clone, Debug, PartialEq)]
pub enum RowOperation {
    Project(Vec<(String, Scalar)>),
    EagerPath {
        link: String,
        leaf: String,
    },
    Sum(Scalar),
    Count {
        distinct: bool,
    },
    CountProjection {
        distinct: bool,
        value: Scalar,
    },
    GroupCount {
        first: Scalar,
        remaining: Vec<Scalar>,
    },
    FirstValue {
        value: Scalar,
        fallback: Scalar,
    },
    FirstRow,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Rows,
    Sum,
    First,
    Count,
    CountDistinct,
    GroupCount,
}

enum Yield {
    Fields(Vec<(String, Scalar)>),
    Value(Scalar),
    FirstValue { value: Scalar, fallback: Scalar },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Query {
    pub binding: String,
    pub accumulate: bool,
    pub source: String,
    pub alias: String,
    pub clauses: Vec<Clause>,
    pub operation: Operation,
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
            let call = match n.as_str() {
                "min" => Some(IntegerBound::Min),
                "max" => Some(IntegerBound::Max),
                "length" => Some(IntegerBound::Length),
                _ => None,
            };
            if let Some(call) = call.filter(|_| t.take("(")) {
                let mut args = vec![sum(t)?];
                if call != IntegerBound::Length {
                    t.expect(",")?;
                    args.push(sum(t)?);
                }
                t.expect(")")?;
                return Ok(Scalar::IntegerBound(call, args));
            }
            if t.take("[") {
                let index = Box::new(sum(t)?);
                t.expect("]")?;
                let mut fields = Vec::new();
                while t.take(".") {
                    fields.push(t.name()?);
                }
                return Ok(Scalar::Indexed {
                    source: n,
                    index,
                    fields,
                });
            }
            let mut path = vec![n];
            while t.take(".") {
                path.push(t.name()?);
            }
            Ok(Scalar::Path(path))
        }
        ScalarOperand::Integer(value) => Ok(Scalar::Integer(value)),
        ScalarOperand::Text(value) => Ok(Scalar::Text(value)),
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
        if !t.take("NULL") {
            t.expect("null")?;
        }
        let Scalar::Path(parts) = left else {
            return Err("SQL NULL predicate requires a declared column".into());
        };
        let [owner, column]: [String; 2] = parts
            .try_into()
            .map_err(|_| "SQL NULL predicate requires a bound row column")?;
        return Ok(Scalar::NullPredicate {
            owner,
            column,
            negated,
        });
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

pub(super) fn parse_range(
    binding: &str,
    accumulate: bool,
    t: &mut Tokens<'_>,
) -> Result<Option<IntegerRange>, String> {
    let kind = if t.take("count") {
        Kind::Count
    } else if t.take("sum") {
        Kind::Sum
    } else if t.take("first") {
        Kind::First
    } else {
        return Ok(None);
    };
    if !t.take("from") || !t.take("[") {
        return Ok(None);
    }

    let lower = sum(t)?;
    t.expect(",")?;
    let upper = sum(t)?;
    t.expect(")")?;
    let alias = if t.take("as") { Some(t.name()?) } else { None };
    let mut filters = Vec::new();
    if t.take("where") {
        if alias.is_none() {
            return Err("range predicates require an index binding".into());
        }
        filters.push(scalar(t)?);
        while t.take("and") {
            filters.push(scalar(t)?);
        }
    }
    let select = match (kind, alias) {
        (Kind::Count, _) if filters.is_empty() => RangeOperation::Count,
        (Kind::Count, Some(alias)) if !accumulate => {
            RangeOperation::FilteredCount(RangeScope { alias, filters })
        }
        (Kind::Sum, None) => RangeOperation::Sum,
        (Kind::Sum, Some(alias)) => {
            t.expect("yield")?;
            let value = scalar(t)?;
            if filters.is_empty()
                && matches!(&value, Scalar::Path(parts)
                if parts.as_slice() == std::slice::from_ref(&alias))
            {
                RangeOperation::Sum
            } else {
                RangeOperation::ProjectedSum {
                    scope: RangeScope { alias, filters },
                    value,
                }
            }
        }
        (Kind::First, Some(alias)) if !accumulate => {
            t.expect("yield")?;
            let value = scalar(t)?;
            t.expect("else")?;
            RangeOperation::First {
                scope: RangeScope { alias, filters },
                value,
                fallback: scalar(t)?,
            }
        }
        _ => return Err("unsupported range operation contract".into()),
    };
    t.end("trailing integer range input")?;
    Ok(Some(IntegerRange {
        binding: binding.into(),
        accumulate,
        lower,
        upper,
        select,
    }))
}

pub(super) fn parse(
    binding: String,
    accumulate: bool,
    t: &mut Tokens<'_>,
) -> Result<Option<Query>, String> {
    let kind = if t.take("sum") {
        Kind::Sum
    } else if t.take("first") {
        Kind::First
    } else if t.take("count") {
        if t.take("distinct") {
            Kind::CountDistinct
        } else if t.take("groups") {
            Kind::GroupCount
        } else {
            Kind::Count
        }
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
    parse_query_tail(binding, accumulate, t, kind, true).map(Some)
}

// Callers have already consumed the aggregate marker and the from keyword.
fn parse_query_tail(
    binding: String,
    accumulate: bool,
    t: &mut Tokens<'_>,
    kind: Kind,
    allow_nested_sum: bool,
) -> Result<Query, String> {
    let source = t.name()?;
    if kind == Kind::First {
        let offset = if t.take("[") {
            let ScalarOperand::Integer(offset) = t.value()? else {
                return Err("expected a nonnegative integer slice offset".into());
            };
            let offset = offset
                .to_biguint()
                .ok_or("slice offset must be nonnegative")?;
            t.expect(":")?;
            t.expect("]")?;
            Some(offset)
        } else {
            None
        };
        if offset.is_some() || t.rest().starts_with("else") {
            t.expect("else")?;
            let default = scalar(t)?;
            t.end("trailing scalar first input")?;
            return Ok(Query {
                binding,
                accumulate,
                source,
                alias: String::new(),
                clauses: vec![],
                operation: Operation::FirstArray {
                    offset: offset.unwrap_or_default(),
                    fallback: default,
                },
            });
        }
    }
    t.expect("as")?;
    let alias = t.name()?;
    if kind == Kind::GroupCount {
        t.expect("by")?;
        let first = scalar(t)?;
        let mut remaining = Vec::new();
        while t.take(",") {
            remaining.push(scalar(t)?);
        }
        t.end("trailing group key clauses")?;
        return Ok(Query {
            binding,
            accumulate,
            source,
            alias,
            clauses: vec![],
            operation: Operation::OnRows(RowOperation::GroupCount { first, remaining }),
        });
    }
    if kind == Kind::Count && t.take("join") {
        let right = t.name()?;
        t.expect("as")?;
        let right_alias = t.name()?;
        if right_alias == alias {
            return Err("join requires two distinct row aliases".into());
        }
        t.expect("on")?;
        let on = scalar(t)?;
        let mut filters = vec![];
        if t.take("where") {
            filters.push(scalar(t)?);
            while t.take("and") {
                filters.push(scalar(t)?);
            }
        }
        t.end("trailing join count input")?;
        return Ok(Query {
            binding,
            accumulate,
            source,
            alias,
            clauses: vec![],
            operation: Operation::JoinCount {
                source: right,
                alias: right_alias,
                on,
                filters,
            },
        });
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
    if t.take("OR") || t.take("or") {
        return Err(
            "OR is not supported in local relation predicates; SQL UNKNOWN must be preserved"
                .into(),
        );
    }
    if matches!(kind, Kind::Count | Kind::CountDistinct) && t.rest().is_empty() {
        return Ok(Query {
            binding,
            accumulate,
            source,
            alias,
            clauses,
            operation: Operation::OnRows(RowOperation::Count {
                distinct: kind == Kind::CountDistinct,
            }),
        });
    }
    t.expect("yield")?;
    let mut nested = Tokens { source: t.rest() };
    let has_nested_sum = kind == Kind::Sum && nested.take("sum") && nested.take("from");
    let projection = if t.take("{") {
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
        Yield::Fields(fields)
    } else if has_nested_sum {
        if !allow_nested_sum {
            return Err("nested sum is limited to two dimensions".into());
        }
        *t = nested;
        let inner = parse_query_tail(String::new(), false, t, Kind::Sum, false)?;
        let value = match inner.operation {
            Operation::OnRows(RowOperation::Sum(value))
                if inner.source == alias && inner.alias != alias =>
            {
                value
            }
            _ => {
                return Err(
                    "nested sum requires a scalar sum from the outer row with a distinct alias"
                        .into(),
                )
            }
        };
        let filters = inner
            .clauses
            .into_iter()
            .map(|clause| match clause {
                Clause::Where(predicate) => Ok(predicate),
                _ => Err("nested sum supports scalar filters only".to_owned()),
            })
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(Query {
            binding,
            accumulate,
            source,
            alias,
            clauses,
            operation: Operation::NestedSum {
                alias: inner.alias,
                filters,
                value,
            },
        });
    } else {
        Yield::Value(scalar(t)?)
    };
    let projection = if kind == Kind::First {
        t.expect("else")?;
        if t.take("null") {
            projection
        } else {
            let Yield::Value(value) = projection else {
                return Err("scalar first fallback requires a scalar projection".into());
            };
            Yield::FirstValue {
                value,
                fallback: scalar(t)?,
            }
        }
    } else {
        projection
    };
    t.end("trailing relation input")
        .map_err(|error| format!("{error}: {}", t.rest()))?;
    let operation = match (kind, projection) {
        (Kind::Rows, Yield::Fields(fields)) => RowOperation::Project(fields),
        (Kind::Rows, Yield::Value(Scalar::Path(path))) => {
            let [owner, link, leaf]: [String; 3] = path
                .try_into()
                .map_err(|_| "eager record projection requires one nullable member hop")?;
            if owner != alias || !clauses.is_empty() {
                return Err(
                    "eager record projection requires an unfiltered source-order path".into(),
                );
            }
            RowOperation::EagerPath { link, leaf }
        }
        (Kind::Sum, Yield::Value(value)) => RowOperation::Sum(value),
        (Kind::Count | Kind::CountDistinct, Yield::Value(value)) => RowOperation::CountProjection {
            distinct: kind == Kind::CountDistinct,
            value,
        },
        (Kind::First, Yield::FirstValue { value, fallback }) => {
            RowOperation::FirstValue { value, fallback }
        }
        (Kind::First, Yield::Value(Scalar::Path(path)))
            if path.as_slice() == std::slice::from_ref(&alias) =>
        {
            RowOperation::FirstRow
        }
        // No input types or acquired values can make this pair legal.
        _ => return Err("incompatible query operation and projection".into()),
    };
    Ok(Query {
        binding,
        accumulate,
        source,
        alias,
        clauses,
        operation: Operation::OnRows(operation),
    })
}
