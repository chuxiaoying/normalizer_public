//! Source-independent syntax and typed operations of a local semantic declaration.
use std::collections::BTreeSet;

pub mod relation;
pub mod stack;
pub mod decimal;

#[derive(Clone, Debug, PartialEq)]
pub enum LocalIr {
    SqlWrite(SqlWrite),
    LocalMapEmpty { binding: String },
    LocalMapPut { binding: String, key: String, value: MapText },
    LocalMapGet { binding: String, source: String, key: String, fallback: MapText },
    ProjectedUnionCount { binding: String, branches: Vec<(String, String)> },
    MemberChoice { binding: String, members: String, empty: MemberChoiceFallback },
    Stack(stack::Action),
    CountRange { binding: String, accumulate: bool, lower: IntegerOperand, upper: IntegerOperand },
    FoldIndexRange {
        binding: String,
        lower: num_bigint::BigInt,
        upper: String,
        skip_before: num_bigint::BigInt,
        stop_after: num_bigint::BigInt,
    },
    Relation(relation::Query),
    Decimal(decimal::Assignment),
}

#[derive(Clone, Debug, PartialEq)]
pub struct SqlWrite {
    pub binding: String,
    pub table: String,
    pub operation: SqlOperation,
    pub assignments: Vec<(String, ScalarOperand)>,
    pub exception: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SqlOperation {
    Insert,
    Update { predicates: Vec<(String, ScalarOperand)> },
    Delete { predicates: Vec<(String, ScalarOperand)> },
}

#[derive(Clone, Debug, PartialEq)]
pub enum MemberChoiceFallback {
    Local(String),
    Integer(num_bigint::BigInt),
}

#[derive(Clone, Debug, PartialEq)]
pub enum MapText {
    Null,
    Literal(String),
    Local(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum ScalarOperand {
    Local(String),
    Text(String),
    Integer(num_bigint::BigInt),
}

/// The closed endpoints of an integer range.
#[derive(Clone, Debug, PartialEq)]
pub enum IntegerOperand {
    Local(String),
    Integer(num_bigint::BigInt),
}

impl IntegerOperand {
    fn parse(value: ScalarOperand, error: &str) -> Result<Self, String> {
        match value {
            ScalarOperand::Local(name) => Ok(Self::Local(name)),
            ScalarOperand::Integer(value) => Ok(Self::Integer(value)),
            ScalarOperand::Text(_) => Err(error.into()),
        }
    }
}

impl SqlWrite {
    pub fn operands(&self) -> impl Iterator<Item = &(String, ScalarOperand)> {
        let predicates: &[(String, ScalarOperand)] = match &self.operation {
            SqlOperation::Update { predicates } | SqlOperation::Delete { predicates } => predicates,
            SqlOperation::Insert => &[],
        };
        self.assignments.iter().chain(predicates)
    }
}

struct Tokens<'a> {
    source: &'a str,
}

impl<'a> Tokens<'a> {
    fn rest(&self) -> &'a str {
        self.source.trim_start()
    }

    fn take(&mut self, token: &str) -> bool {
        let Some(rest) = self.rest().strip_prefix(token) else {
            return false;
        };
        self.source = rest;
        true
    }

    fn expect(&mut self, token: &str) -> Result<(), String> {
        self.take(token)
            .then_some(())
            .ok_or_else(|| format!("expected {token}"))
    }

    fn end(&self, diagnostic: &str) -> Result<(), String> {
        self.rest().is_empty().then_some(()).ok_or_else(|| diagnostic.into())
    }

    fn bound(&mut self, expected: &str, diagnostic: &str) -> Result<(), String> {
        (self.name()? == expected).then_some(()).ok_or_else(|| diagnostic.into())
    }

    fn name(&mut self) -> Result<String, String> {
        let source = self.rest();
        let n = source
            .bytes()
            .take_while(|b| b.is_ascii_alphanumeric() || *b == b'_')
            .count();
        if n == 0 || source.as_bytes()[0].is_ascii_digit() {
            return Err("expected identifier".into());
        }
        self.source = &source[n..];
        Ok(source[..n].to_owned())
    }

    fn value(&mut self) -> Result<ScalarOperand, String> {
        let source = self.rest();
        if source.starts_with('"') {
            let mut chars = source[1..].char_indices();
            let mut value = String::new();
            while let Some((i, c)) = chars.next() {
                match c {
                    '"' => {
                        self.source = &source[i + 2..];
                        return Ok(ScalarOperand::Text(value));
                    }
                    '\\' => {
                        let (_, escaped) = chars.next().ok_or("unterminated escape")?;
                        if !matches!(escaped, '\\' | '"') {
                            return Err("unsupported text escape".into());
                        }
                        value.push(escaped);
                    }
                    _ => value.push(c),
                }
            }
            return Err("unterminated string".into());
        }
        if source.starts_with(|c: char| c.is_ascii_digit() || c == '-') {
            let n = source
                .trim_start_matches('-')
                .bytes()
                .take_while(u8::is_ascii_digit)
                .count()
                + usize::from(source.starts_with('-'));
            let value = source[..n].parse().map_err(|_| "invalid integer literal")?;
            self.source = &source[n..];
            return Ok(ScalarOperand::Integer(value));
        }
        self.name().map(ScalarOperand::Local)
    }
}

pub fn parse(input: &str) -> Result<LocalIr, String> {
    let mut tokens = Tokens { source: input };
    let binding = tokens.name()?;
    // Destination bindings name Java locals, never the instance receiver.
    // SQL aliases and operand names are parsed separately.
    if binding == "this" {
        return Err("[NORM-UNSUPPORTED-LOCAL-IR] cannot bind the Java receiver this".into());
    }
    let accumulate = tokens.take("+=");
    if !accumulate && !tokens.take("=") {
        return Err("[NORM-UNSUPPORTED-LOCAL-IR] a local definition requires = or +=".into());
    }
    if !accumulate {
        let mut exact = Tokens { source: tokens.source };
        if let Some(assignment) = decimal::parse(&binding, &mut exact)? {
            return Ok(LocalIr::Decimal(assignment));
        }
    }
    let mut ordered = Tokens { source: tokens.source };
    if let Some(action) = stack::parse(&binding, &mut ordered)? {
        if accumulate { return Err("local stack uses only the = binding".into()); }
        return Ok(LocalIr::Stack(action));
    }
    if let Some(source) = tokens.rest().strip_prefix("value")
        .and_then(|tail| tail.trim_start().strip_prefix('.'))
    {
        let mut map = Tokens { source };
        let key = |t: &mut Tokens<'_>| match t.value()? {
            ScalarOperand::Text(text) => Ok(text),
            _ => Err("local map key must be a text literal".to_owned()),
        };
        let value = |t: &mut Tokens<'_>| match t.value()? {
            ScalarOperand::Local(name) if name == "null" => Ok(MapText::Null),
            ScalarOperand::Local(name) => Ok(MapText::Local(name)),
            ScalarOperand::Text(text) => Ok(MapText::Literal(text)),
            _ => Err("local map requires a nullable text value".to_owned()),
        };
        let operation = match map.name().ok().as_deref() {
            Some("map_empty") => {
                map.expect("(")?; map.expect("text")?; map.expect(",")?;
                map.expect("text")?; map.expect("?")?;
                LocalIr::LocalMapEmpty { binding }
            }
            Some("map_put") => {
                map.expect("(")?;
                let target = map.name()?;
                if target != binding { return Err("local map update cannot alias to another binding".into()); }
                map.expect(",")?;
                let key = key(&mut map)?;
                map.expect(",")?;
                LocalIr::LocalMapPut { binding, key, value: value(&mut map)? }
            }
            Some("map_get_or") => {
                map.expect("(")?;
                let source = map.name()?;
                map.expect(",")?;
                let key = key(&mut map)?;
                map.expect(",")?;
                LocalIr::LocalMapGet { binding, source, key, fallback: value(&mut map)? }
            }
            _ if accumulate => return Err("[NORM-UNSUPPORTED-LOCAL-IR] unknown accumulating local operation".into()),
            _ => return Err("[NORM-UNSUPPORTED-LOCAL-IR] unknown local operation".into()),
        };
        map.expect(")")?;
        if accumulate || !map.rest().is_empty() { return Err("local map requires a closed = definition".into()); }
        return Ok(operation);
    }
    let mut union = Tokens { source: tokens.source };
    if union.take("count") && union.take("distinct") && union.take("union") {
        if accumulate { return Err("projected union initializes its count with =".into()); }
        let mut branches = Vec::new();
        loop {
            let source = union.name()?;
            union.expect(".")?;
            branches.push((source, union.name()?));
            if !union.take(",") { break; }
        }
        if branches.len() < 2 || !union.rest().is_empty() {
            return Err("projected union needs at least two closed branches".into());
        }
        return Ok(LocalIr::ProjectedUnionCount { binding, branches });
    }
    let mut range = Tokens { source: tokens.source };
    if range.take("count") && range.take("from") && range.take("[") {
        tokens = range;
        let lower = tokens.value()?;
        tokens.expect(",")?;
        let upper = tokens.value()?;
        tokens.expect(")")?;
        tokens.expect("as")?;
        tokens.name()?; // The binder ranges over each integer; count needs no projection.
        tokens.end("trailing count-range clauses")?;
        let lower = IntegerOperand::parse(lower, "count range endpoints must be integers")?;
        let upper = IntegerOperand::parse(upper, "count range endpoints must be integers")?;
        return Ok(LocalIr::CountRange { binding, accumulate, lower, upper });
    }
    if tokens.take("choose") {
        if accumulate { return Err("member choice must initialize its binding with =".into()); }
        tokens.expect("from")?;
        let members = tokens.name()?;
        tokens.expect("else")?;
        let empty = match tokens.value()? {
            ScalarOperand::Local(name) => MemberChoiceFallback::Local(name),
            ScalarOperand::Integer(value) => MemberChoiceFallback::Integer(value),
            ScalarOperand::Text(_) => return Err("member choice requires one integer empty fallback".into()),
        };
        tokens.end("trailing member choice clauses")?;
        return Ok(LocalIr::MemberChoice { binding, members, empty });
    }
    if tokens.take("fold") {
        if accumulate { return Err("indexed fold updates its existing accumulator; use =".into()); }
        let integer = |t: &mut Tokens<'_>| match t.value()? {
            ScalarOperand::Integer(n) => Ok(n),
            _ => Err("expected a literal integer in indexed fold".to_owned()),
        };
        tokens.expect("[")?;
        let lower = integer(&mut tokens)?;
        tokens.expect(",")?;
        let upper = tokens.name()?;
        tokens.expect(")")?;
        tokens.expect("as")?;
        let index = tokens.name()?;
        if index == binding || upper == index || upper == binding {
            return Err("indexed fold needs separate bound, accumulator and index".into());
        }
        tokens.expect("when")?;
        tokens.bound(&index, "unbound indexed fold variable")?;
        tokens.expect("<")?;
        let skip_before = integer(&mut tokens)?;
        tokens.expect("continue")?;
        tokens.expect("when")?;
        tokens.bound(&index, "unbound indexed fold variable")?;
        tokens.expect(">")?;
        let stop_after = integer(&mut tokens)?;
        tokens.expect("break")?;
        tokens.expect("yield")?;
        tokens.bound(&index, "unbound indexed fold variable")?;
        tokens.end("trailing indexed fold input")?;
        return Ok(LocalIr::FoldIndexRange { binding, lower, upper, skip_before, stop_after });
    }
    if let Some(query) = relation::parse(binding.clone(), accumulate, &mut tokens)? {
        return Ok(LocalIr::Relation(query));
    }
    if accumulate {
        return Err("[NORM-UNSUPPORTED-LOCAL-IR] unknown accumulating local operation".into());
    }
    let insert = tokens.take("insert");
    let delete = !insert && tokens.take("delete");
    if !insert && !delete && !tokens.take("update") {
        return Err("[NORM-UNSUPPORTED-LOCAL-IR] unknown local operation".into());
    }
    if insert { tokens.expect("into")?; }
    if delete { tokens.expect("from")?; }
    let table = tokens.name()?;
    let alias = if !insert && tokens.take("as") {
        tokens.name()?
    } else {
        table.clone()
    };
    let mut assignments = Vec::new();
    if !delete {
        tokens.expect(if insert { "values" } else { "set" })?;
        tokens.expect("{")?;
        let mut columns = BTreeSet::new();
        loop {
            let name = tokens.name()?;
            if !columns.insert(name.clone()) {
                return Err(format!("duplicate column {name}"));
            }
            tokens.expect(":")?;
            assignments.push((name, tokens.value()?));
            if tokens.take("}") {
                break;
            }
            tokens.expect(",")?;
        }
    }
    let operation = if insert {
        SqlOperation::Insert
    } else {
        let mut predicates = Vec::new();
        if tokens.take("where") {
            loop {
                tokens.bound(&alias, "unbound table alias in where")?;
                tokens.expect(".")?;
                let column = tokens.name()?;
                tokens.expect("==")?;
                predicates.push((column, tokens.value()?));
                if !tokens.take("and") {
                    break;
                }
            }
        }
        if delete {
            if predicates.is_empty() { return Err("delete requires an explicit predicate".into()); }
            SqlOperation::Delete { predicates }
        } else {
            SqlOperation::Update { predicates }
        }
    };
    tokens.expect("returning")?;
    tokens.expect("affected_rows")?;
    tokens.expect("atomic")?;
    tokens.expect("throws")?;
    let exception = tokens.name()?;
    tokens.end("trailing input in SQL write")?;
    Ok(LocalIr::SqlWrite(SqlWrite {
        binding,
        table,
        operation,
        assignments,
        exception,
    }))
}

#[cfg(test)]
mod map_namespace_contract {
    use super::*;

    #[test]
    fn a_map_operation_uses_its_whole_token_in_the_value_namespace() {
        assert!(matches!(parse("labels = value . map_empty(text,text?)").unwrap(),
            LocalIr::LocalMapEmpty { binding } if binding == "labels"));
        assert!(matches!(parse("choice = value").unwrap(), LocalIr::Decimal(_)));
        for expression in ["value.map_putter(labels,\"key\",null)", "value.entries(labels)"] {
            assert_eq!(parse(&format!("labels = {expression}")).unwrap_err(),
                "[NORM-UNSUPPORTED-LOCAL-IR] unknown local operation");
        }
    }
}
