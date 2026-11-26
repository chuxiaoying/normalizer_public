//! Composition of typed local queries with the current Java/action frame.
//! A view binds projected row fields. Later clauses substitute those values
//! into the shared relation algebra, preserving the original table source.
use super::*;
use normalizer_ir::local::{
    relation::{Clause, Op, Operation, Query, RowOperation, Scalar},
};

#[derive(Clone, PartialEq)]
pub(super) struct Rows {
    input: RelationExpr,
    fields: BTreeMap<String, Value>,
    ordering: Vec<Expr>,
}

/// A selected row has already passed the same ordering admission as FIRST.
pub(super) struct OrderedRows {
    input: RelationExpr,
    fields: BTreeMap<String, Value>,
    first_key: Expr,
    remaining_keys: Vec<Expr>,
}

#[derive(Default)]
pub(super) struct Relations {
    pub(super) tables: BTreeMap<String, BTreeMap<String, String>>,
    pub(super) record_inputs: BTreeMap<String, String>,
    pub(super) first_rows: BTreeMap<i64, OrderedRows>,
}

impl Engine<'_> {
    /// Only declared base tables filtered by closed, typed expressions in the
    /// current source frame are admitted. In particular, raw E/T values or
    /// composite observations cannot acquire this proof through their name.
    fn closed_count_source(&self, relation: &RelationExpr) -> bool {
        let mut predicates = Vec::new();
        let mut base = relation;
        while let RelationExpr::Select { predicate, input } = base {
            predicates.push(predicate);
            base = input;
        }
        let RelationExpr::Base { name, alias } = base else { return false; };
        let Some(columns) = self.source_relations.tables.get(name).filter(|_| name == alias) else {
            return false;
        };
        let Some(entry) = self.selected_entry.and_then(|id| self.methods.get(id)) else {
            return false;
        };
        fn closed(expr: &Expr, table: &str, columns: &BTreeMap<String, String>, entry: &simulator_parser::JavaMethod) -> bool {
            if let Some(literal) = expr.as_lit() {
                return matches!(literal, math::Lit::Int(_) | math::Lit::BigInt(_) | math::Lit::Bool(_) | math::Lit::Str(_) | math::Lit::Null);
            }
            if let Some(row) = expr.as_row_value() {
                return row.split_once('.').is_some_and(|(owner, field)|
                    owner == table && columns.contains_key(field));
            }
            if let Some(SymVarOrigin::Parameter { name, primitive_type }) = expr.as_sym_var() {
                return entry.params().iter().any(|(ty, parameter)| parameter == name
                    && match primitive_type.as_str() {
                        "integer" => crate::normalized::integral(ty),
                        "boolean" => matches!(ty, JavaType::Primitive(t) if t == "boolean"),
                        _ => false,
                    });
            }
            if let Some((op, left, right)) = expr.as_binop() {
                return matches!(op, BinOpKind::And | BinOpKind::Or | BinOpKind::Eq | BinOpKind::Ne
                    | BinOpKind::Lt | BinOpKind::Le | BinOpKind::Gt | BinOpKind::Ge
                    | BinOpKind::Add | BinOpKind::Sub | BinOpKind::Mul)
                    && closed(left, table, columns, entry) && closed(right, table, columns, entry);
            }
            if let Some((op, operand)) = expr.as_unary_op() {
                return matches!(op, math::UnaryOpKind::Not | math::UnaryOpKind::Neg)
                    && closed(operand, table, columns, entry);
            }
            false
        }
        predicates.into_iter().all(|p| closed(p, name, columns, entry.syntax))
    }

    fn query_row_declaration(&self, decl: &JavaType) -> Result<&simulator_parser::JavaClass> {
        let JavaType::Generic(container, args) = decl else {
            return Err(fail(format!(
                "query output requires List<Row>, found {decl:?}"
            )));
        };
        let [JavaType::Class(name)] = args.as_slice() else {
            return Err(fail("missing declared query row type"));
        };
        need(
            matches!(container.as_str(), "List" | "java.util.List"),
            "expected List<Row>",
        )?;
        self.program.find_class(name).ok_or_else(|| fail("undeclared row type"))
    }

    fn query_row_types(&self, row: &simulator_parser::JavaClass) -> Result<BTreeMap<String, Ty>> {
        row.fields()
            .iter()
            .map(|f| Ok((f.name().into(), self.project.parameter_type(f.java_type())?)))
            .collect()
    }

    fn table_rows(&mut self, query: &Query, decl: &JavaType, env: &Env) -> Result<Rows> {
        let row = self.query_row_declaration(decl)?;
        let types = self.query_row_types(row)?;
        let Operation::OnRows(RowOperation::Project(fields)) = &query.operation else {
            return Err(fail("table rows require a record projection"));
        };
        let nullable_fields: std::collections::BTreeSet<_> = row.fields().iter()
            .filter(|field| crate::normalized::nullable_text_field(row, field))
            .map(|field| field.name()).collect();
        let mut nullable_columns = std::collections::BTreeSet::new();
        let mut columns = BTreeMap::new();
        let mut declare = |path: &Scalar, ty: Ty| -> Result<()> {
            if let Scalar::Path(parts) = path {
                if let [root, column] = parts.as_slice() {
                    if root == &query.alias {
                        if let Some(old) = columns.insert(column.clone(), ty.clone()) {
                            need(old == ty, "incompatible column declarations")?;
                        }
                    }
                }
            }
            Ok(())
        };
        for (name, expr) in fields {
            if nullable_fields.contains(name.as_str()) {
                let Scalar::Path(path) = expr else {
                    return Err(fail("nullable table field requires a direct column"));
                };
                let [alias, column] = path.as_slice() else {
                    return Err(fail("nullable table field requires a qualified column"));
                };
                need(alias == &query.alias, "unbound nullable table column alias")?;
                nullable_columns.insert(column.clone());
            }
            declare(
                expr,
                types
                    .get(name)
                    .ok_or_else(|| fail("unknown projected row field"))?
                    .clone(),
            )?;
        }
        for clause in &query.clauses {
            if let Clause::Where(Scalar::Binary(_, a, b)) = clause {
                for (subject, evidence) in [(&**a, &**b), (&**b, &**a)] {
                    let ty = match evidence {
                        Scalar::Integer(_) => Some(Ty::Integer),
                        Scalar::Text(_) => Some(Ty::Text),
                        Scalar::Path(path) if path.len() == 1 => env.get(&path[0]).map(Value::ty),
                        _ => None,
                    };
                    if let Some(ty) = ty {
                        declare(subject, ty)?;
                    }
                }
            }
        }
        let table = self
            .source_relations
            .tables
            .entry(query.source.clone())
            .or_default();
        for (name, ty) in &columns {
            let business_type = match ty {
                Ty::Integer => "number",
                Ty::Text if nullable_columns.contains(name) => "text?",
                Ty::Text => "text",
                Ty::Bool => "bool",
                _ => return Err(fail("unsupported table column type")),
            };
            if let Some(existing) = table.insert(name.clone(), business_type.into()) {
                need(existing == business_type, "inconsistent table column type")?;
            }
        }
        Ok(Rows {
            input: RelationExpr::base(&query.source, &query.source),
            fields: columns
                .into_iter()
                .map(|(n, t)| {
                    let value = b::row_value(format!("{}.{}", query.source, n));
                    (n, Value::Term(t, value))
                })
                .collect(),
            ordering: vec![],
        })
    }

    fn query_value(
        &self,
        scalar: &Scalar,
        alias: &str,
        fields: &BTreeMap<String, Value>,
        locals: &Env,
        guard: &Expr,
    ) -> Result<Value> {
        match scalar {
            Scalar::Bool(value) => Ok(Value::Term(Ty::Bool, b::lit_bool(*value))),
            Scalar::Text(text) => Ok(Value::Term(Ty::Text, b::lit_str(text))),
            Scalar::Integer(n) => {
                Ok(Value::Term(Ty::Integer, b::lit_integer(n.clone())))
            }
            Scalar::Path(path) => {
                let mut parts = path.iter();
                let root = parts.next().ok_or_else(|| fail("empty row path"))?;
                let mut value = if root == alias && path.len() == 1 && fields.contains_key("") {
                    fields[""].clone()
                } else if root == alias {
                    fields
                        .get(parts.next().ok_or_else(|| fail("expected a row field"))?)
                        .cloned()
                        .ok_or_else(|| fail("unknown projected field"))?
                } else {
                    if let Some(value) = locals.get(&path.join(".")) {
                        return Ok(value.clone());
                    }
                    locals
                        .get(root)
                        .cloned()
                        .ok_or_else(|| fail(format!("undeclared query input {root}")))?
                };
                for member in parts {
                    value = if self.source_objects.owns(&value.scalar()?) {
                        self.read_source_field_on_path(&value.scalar()?, member, guard)?
                    } else {
                        values::field(value, member)?
                    };
                }
                Ok(value)
            }
            Scalar::NullPredicate { value, negated } => {
                let Scalar::Path(parts) = value.as_ref() else {
                    return Err(fail("SQL NULL predicate requires a declared column"));
                };
                need(matches!(parts.as_slice(), [owner, column] if owner == alias && fields.contains_key(column)),
                    "SQL NULL predicate requires a bound row column")?;
                let value = self.query_value(value, alias, fields, locals, guard)?.scalar()?;
                need(value.as_row_value().is_some(), "SQL NULL predicate requires a table column")?;
                let op = if *negated { b::ne } else { b::eq };
                Ok(Value::Term(Ty::Bool, op(value, b::lit_null())))
            }
            Scalar::Binary(op, left, right) => {
                let left = self.query_value(left, alias, fields, locals, guard)?;
                let right = self.query_value(right, alias, fields, locals, guard)?;
                need(left.ty() == right.ty(), "incompatible query operands")?;
                let binary: fn(Expr, Expr) -> Expr = match op {
                    Op::Add => b::add,
                    Op::Sub => b::sub,
                    Op::Mul => b::mul,
                    Op::Eq => b::eq,
                    Op::Ne => b::ne,
                    Op::Lt => b::lt,
                    Op::Le => b::le,
                    Op::Gt => b::gt,
                    Op::Ge => b::ge,
                };
                need(
                    matches!(op, Op::Eq | Op::Ne) || left.ty() == Ty::Integer,
                    "expected an exact integer query operand",
                )?;
                let t = if matches!(op, Op::Add | Op::Sub | Op::Mul) {
                    Ty::Integer
                } else {
                    Ty::Bool
                };
                Ok(Value::Term(t, math::normalize(binary(left.scalar()?, right.scalar()?))))
            }
        }
    }

    pub(super) fn projected_union_count(
        &self, binding: &str, sources: &[(String, String)], env: &mut Env,
    ) -> Result<()> {
        let branches = sources.iter().map(|(source, field)| {
            let Some(Value::Rows(rows)) = env.get(source) else {
                return Err(fail("projected union needs a declared table row source"));
            };
            let RelationExpr::Base {name, alias} = &rows.input else {
                return Err(fail("projected union needs direct table rows without filters"));
            };
            need(rows.ordering.is_empty() && self.source_relations.tables.contains_key(name),
                "projected union requires an unordered read-only table")?;
            let Some(Value::Term(Ty::Integer, expression)) = rows.fields.get(field) else {
                return Err(fail("projected union field must be a declared nonnullable integer"));
            };
            let column = expression.as_row_value().and_then(|path| path.split_once('.'))
                .filter(|(root, _)| *root == alias)
                .map(|(_, column)| column)
                .filter(|column| self.source_relations.tables[name].get(*column).is_some_and(|ty| ty == "number"))
                .ok_or_else(|| fail("projected union field must be a direct nonnullable table column"))?;
            Ok(RelationExpr::project(b::row_value(format!("{name}.{column}")), "value",
                RelationExpr::base(name, name)))
        }).collect::<Result<Vec<_>>>()?;
        let count = r::relation_count(RelationExpr::union(branches));
        env.insert(binding.into(), Value::Term(Ty::Integer, count));
        Ok(())
    }

    pub(super) fn execute_query(
        &mut self,
        query: &Query,
        decl: Option<&JavaType>,
        env: &mut Env,
        guard: &Expr,
    ) -> Result<()> {
        if let Some(Value::Sequence { name, .. }) = env.get(&query.source) {
            need(!self.source_array_writes.contains_key(name),
                "use a supported source post-write traversal; local query cannot read a mutated array yet")?;
        }
        if let Operation::OnRows(RowOperation::EagerPath(parts)) = &query.operation {
            if matches!(decl, Some(JavaType::Array(item)) if crate::normalized::integral(item)) {
                let [alias, link, leaf] = parts.as_slice() else {
                    return Err(fail("eager record projection requires one nullable member hop"));
                };
                need(alias == &query.alias && query.clauses.is_empty(),
                    "eager record projection requires an unfiltered source-order path")?;
                let Some(Value::Sequence { item: Ty::Ref, name, .. }) =
                    env.get(&query.source) else { return Err(fail("eager projection requires a directly bound record array")); };
                let root = name.clone();
                let row = self.source_relations.record_inputs.get(&root).and_then(|owner| self.program.find_class(owner))
                    .ok_or_else(|| fail("eager projection requires its declared record element"))?;
                let member = row.fields().iter().find(|field| field.name() == link)
                    .ok_or_else(|| fail("undeclared eager projection member"))?;
                need(crate::normalized::source_nullable_field(row, member),
                    "eager projection member requires an explicit nullable record declaration")?;
                let child = self.project.source_owner(member.java_type()).and_then(|owner| self.program.find_class(owner))
                    .ok_or_else(|| fail("eager projection member must be a declared record"))?;
                need(child.fields().iter().any(|field| field.name() == leaf && crate::normalized::integral(field.java_type())),
                    "eager projection leaf must be a declared integer")?;
                let base = RelationExpr::base(&root, "r");
                let missing = b::eq(b::row_value(format!("r.{link}")), b::lit_null());
                // This is the completion of the entire projection, not of a single selected row.
                let exception = r::relation_exists(RelationExpr::select(missing, base.clone()));
                self.throw_source_on_path(b::and(guard.clone(), exception), "java.lang.NullPointerException");
                env.insert(query.binding.clone(), Value::EagerIntProjection {
                    base, selected: b::row_value(format!("r.{link}.{leaf}")),
                });
                return Ok(());
            }
        }
        let operation = match &query.operation {
            Operation::FirstArray { offset, fallback } => {
                let Some(Value::Sequence { item: Ty::Integer, name, .. }) =
                    env.get(&query.source) else { return Err(fail("scalar first needs a bound numeric array")); };
                let relation = RelationExpr::base(name, "value");
                let default = self.query_value(fallback, "", &BTreeMap::new(), env, guard)?;
                need(default.ty() == Ty::Integer, "scalar first requires a numeric fallback")?;
                need(decl.is_none_or(crate::normalized::integral), "scalar first requires a numeric local")?;
                let index = b::lit_integer(offset.clone());
                let length = r::relation_count(relation.clone());
                let none = if offset.bits() == 0 { b::eq(length, index.clone()) } else { b::le(length, index.clone()) };
                let nth = r::relation_nth_value(relation, index, b::lit_null());
                let value = b::case_when(vec![(none, default.scalar()?), (b::lit_bool(true), nth)]);
                env.insert(query.binding.clone(), Value::Term(Ty::Integer, value));
                return Ok(());
            }
            Operation::NestedSum { alias, clauses, value } => {
                let Some(Value::Sequence { item: Ty::Seq(element), name, .. }) = env.get(&query.source) else {
                    return Err(fail("nested sum requires a directly bound two-dimensional array"));
                };
                need(**element == Ty::Integer,
                    "nested sum requires exact integer elements")?;
                let fresh = |prefix: &str| {
                    let mut name = prefix.to_owned();
                    while env.contains_key(&name) || self.source_relations.tables.contains_key(&name) {
                        name.push('_');
                    }
                    name
                };
                let group = fresh("grp");
                let item = fresh("value");
                let outer = RelationExpr::base(name, &group);
                let mut inner_relation = RelationExpr::vector(b::row_value(&group), &item);
                let row = BTreeMap::from([(String::new(), Value::Term(Ty::Integer, b::row_value(&item)))]);
                // The outer binding shadows any equally named entry local, but
                // is an array here, not an integer projection.
                let mut inner_locals = env.clone();
                inner_locals.remove(&query.alias);
                for clause in clauses {
                    let Clause::Where(p) = clause else {
                        return Err(fail("nested sum supports scalar filters only"));
                    };
                    let predicate = self.query_value(p, alias, &row, &inner_locals, guard)?;
                    need(predicate.ty() == Ty::Bool, "inner where requires bool")?;
                    inner_relation = RelationExpr::select(predicate.scalar()?, inner_relation);
                }
                need(query.clauses.is_empty(), "nested sum outer clauses require a group-valued operation")?;
                let element = self.query_value(value, alias, &row, &inner_locals, guard)?;
                need(element.ty() == Ty::Integer, "sum requires exact integers")?;
                need(decl.is_none_or(crate::normalized::integral), "nested sum requires a numeric local")?;
                let summed = r::relation_sum(outer, r::relation_sum(inner_relation, element.scalar()?));
                let summed = if query.accumulate {
                    let prior = env.get(&query.binding).ok_or_else(|| fail("+= needs an initialized local"))?;
                    need(prior.ty() == Ty::Integer, "+= requires a number")?;
                    b::add(prior.scalar()?, summed)
                } else { summed };
                env.insert(query.binding.clone(), Value::Term(Ty::Integer, math::normalize(summed)));
                return Ok(());
            }
            Operation::JoinCount { source: other, alias: rhs, on, filters } => {
                let mut names = env.clone();
                let mut bind = |source: &str, alias: &str| -> Result<RelationExpr> {
                    need(!env.contains_key(alias), "join alias would shadow a local value")?;
                    let Some(Value::Rows(rows)) = env.get(source) else {
                        return Err(fail("join requires two typed table bindings"));
                    };
                    let RelationExpr::Base { name, alias: bound } = &rows.input else {
                        return Err(fail("join count currently requires unfiltered table declarations"));
                    };
                    need(name == bound && self.source_relations.tables.contains_key(name),
                        "join count requires a declared table")?;
                    for (field, value) in &rows.fields {
                        let value = value.scalar()?;
                        let (table, column) = value.as_row_value().and_then(|p| p.split_once('.'))
                            .ok_or_else(|| fail("join requires direct table columns"))?;
                        need(table == name && field == column, "join table declaration must keep physical column names")?;
                        need(self.source_relations.tables[name][column] != "text?",
                            "join count currently requires non-nullable columns")?;
                        names.insert(format!("{alias}.{field}"),
                            Value::Term(rows.fields[field].ty(), b::row_value(format!("{alias}.{field}"))));
                    }
                    Ok(RelationExpr::base(name, alias))
                };
                let outer = bind(&query.source, &query.alias)?;
                let mut inner = bind(other, rhs)?;
                let predicate = |expr| {
                    let value = self.query_value(expr, "", &BTreeMap::new(), &names, guard)?;
                    need(value.ty() == Ty::Bool, "join predicate must be bool")?;
                    value.scalar()
                };
                let on = predicate(on)?;
                let dependency = |predicate: &Expr| join_dependency(predicate, &query.alias, rhs);
                need(dependency(&on) == 3, "join ON must depend on both typed rows")?;
                inner = RelationExpr::select(on, inner);
                for clause in filters {
                    let filter = predicate(clause)?;
                    need(dependency(&filter) != 3, "join WHERE may filter one row per clause")?;
                    inner = RelationExpr::select(filter, inner);
                }
                need(decl.is_none_or(crate::normalized::integral), "join count needs a numeric local")?;
                // Correlated sum counts every matching pair, with multiplicity.
                let result = r::relation_sum(outer, r::relation_sum(inner, b::lit_int(1)));
                env.insert(query.binding.clone(), Value::Term(Ty::Integer, result));
                return Ok(());
            }
            Operation::OnRows(operation) => operation,
        };
        let mut locals = env.clone();
        let mut rows = match env.get(&query.source) {
            Some(Value::Rows(rows)) => (**rows).clone(),
            Some(Value::Sequence { item: Ty::Integer, name, .. }) => {
                Rows {
                    input: RelationExpr::base(name, "value"),
                    fields: BTreeMap::from([(String::new(), Value::Term(Ty::Integer, b::acquired_integer_row("value")))]),
                    ordering: vec![],
                }
            }
            Some(Value::Sequence { item: Ty::Ref, name, .. }) => {
                let owner = self.source_relations.record_inputs.get(name)
                    .ok_or_else(|| fail("collection requires a declared record element"))?;
                let row = self.program.find_class(owner).ok_or_else(|| fail("missing collection row declaration"))?;
                let fields = row.fields().iter().map(|field| {
                    let ty = self.project.parameter_type(field.java_type())?;
                    need(matches!(ty, Ty::Integer | Ty::Bool | Ty::Text), "unsupported collection row field type")?;
                    let value = b::row_value(format!("{}.{}", query.alias, field.name()));
                    Ok((field.name().into(), Value::Term(ty, value)))
                }).collect::<Result<_>>()?;
                Rows { input: RelationExpr::base(name, &query.alias), fields, ordering: vec![] }
            }
            Some(vector) => {
                let Ty::Vector(item) = vector.ty() else {
                    return Err(fail("expected a row vector"));
                };
                let Ty::Record(fields) = *item else {
                    return Err(fail("row vector element requires named fields"));
                };
                let fields = fields
                    .into_iter()
                    .map(|(name, ty)| {
                        let field = b::row_value(format!("{}.{}", query.alias, name));
                        (name, Value::Term(ty, field))
                    })
                    .collect();
                Rows {
                    input: RelationExpr::vector(
                        vector.scalar()?,
                        &query.alias,
                    ),
                    fields,
                    ordering: vec![],
                }
            }
            None if operation.introduces_table() => self.table_rows(
                query,
                decl.ok_or_else(|| fail("table query needs a row declaration"))?,
                env,
            )?,
            _ => return Err(fail("aggregate source is not bound")),
        };
        for clause in &query.clauses {
            match clause {
                Clause::Where(expr) => {
                    let val = self.query_value(expr, &query.alias, &rows.fields, &locals, guard)?;
                    need(val.ty() == Ty::Bool, "where requires bool")?;
                    rows.input = RelationExpr::select(val.scalar()?, rows.input);
                }
                Clause::Let(name, expr) => {
                    need(
                        !locals.contains_key(name) && name != &query.alias,
                        "duplicate local query binding",
                    )?;
                    locals.insert(
                        name.clone(),
                        self.query_value(expr, &query.alias, &rows.fields, &locals, guard)?,
                    );
                }
                Clause::Order(keys) => {
                    need(rows.ordering.is_empty(), "multiple order clauses")?;
                    rows.ordering = keys
                        .iter()
                        .map(|key| {
                            let value =
                                self.query_value(key, &query.alias, &rows.fields, &locals, guard)?;
                            need(
                                value.ty() == Ty::Integer,
                                "ordered query requires numeric sort keys",
                            )?;
                            value.scalar()
                        })
                        .collect::<Result<_>>()?;
                }
            }
        }
        let result = match operation {
            RowOperation::Project(fields) => {
                rows.fields = fields
                    .iter()
                    .map(|(k, e)| {
                        Ok((
                            k.clone(),
                            self.query_value(e, &query.alias, &rows.fields, &locals, guard)?,
                        ))
                    })
                    .collect::<Result<_>>()?;
                if let Some(declared) = decl {
                    let types = self.query_row_types(self.query_row_declaration(declared)?)?;
                    need(
                        types.len() == rows.fields.len()
                            && rows
                                .fields
                                .iter()
                                .all(|(name, value)| types.get(name) == Some(&value.ty())),
                        "row projection does not match declared fields and types",
                    )?;
                }
                env.insert(query.binding.clone(), Value::Rows(Box::new(rows)));
                return Ok(());
            }
            RowOperation::Sum(expr) => {
                let value = self.query_value(expr, &query.alias, &rows.fields, &locals, guard)?;
                need(value.ty() == Ty::Integer, "sum requires business numbers")?;
                let sum = r::relation_sum(rows.input, value.scalar()?);
                let sum = if query.accumulate {
                    let prior = env
                        .get(&query.binding)
                        .ok_or_else(|| fail("+= needs an initialized local"))?;
                    need(prior.ty() == Ty::Integer, "+= requires a number")?;
                    b::add(prior.scalar()?, sum)
                } else {
                    sum
                };
                Value::Term(Ty::Integer, math::normalize(sum))
            }
            RowOperation::Count { distinct } if rows.fields.contains_key("") =>
            {
                let row = rows.fields[""].scalar()?;
                let count = if *distinct {
                    r::relation_count_distinct(rows.input, row)
                } else {
                    r::relation_count_nonnull(rows.input, row)
                };
                Value::Term(Ty::Integer, math::normalize(count))
            }
            RowOperation::Count { distinct: false } =>
            {
                // This is the unprojected, no-default cardinality operation.
                // Preserve its existing SUM(SELECT 1) business representation,
                // but issue a bound only here, not from generic SUM expressions.
                let certified = self.project.direct_source && self.closed_count_source(&rows.input);
                let value = math::normalize(r::relation_sum(rows.input, b::lit_int(1)));
                if certified {
                    Value::IntegerFloor { value, lower: 0.into(), domain: self.normal_path(guard.clone()) }
                } else {
                    Value::Term(Ty::Integer, value)
                }
            }
            RowOperation::GroupCount(keys) => {
                // Each group is one distinct *tuple* of evaluated keys, not
                // independent distinct counts and not the source row count.
                let RelationExpr::Base { name, alias } = &rows.input else {
                    return Err(fail("group count currently requires an unfiltered table declaration"));
                };
                need(name == alias && self.source_relations.tables.contains_key(name),
                    "group count requires a declared table")?;
                let keys = keys.iter().map(|key| {
                    let value = self.query_value(key, &query.alias, &rows.fields, &locals, guard)?;
                    let value = value.scalar()?;
                    need(value.as_row_value().is_some_and(|path| path.split_once('.').is_some_and(
                        |(table, column)| table == name && self.source_relations.tables[name].contains_key(column))),
                        "group key requires a declared table column")?;
                    Ok(value)
                }).collect::<Result<Vec<_>>>()?;
                let distinct = RelationExpr::distinct(RelationExpr::project(b::array_lit(keys), "groups", rows.input));
                Value::Term(Ty::Integer, r::relation_sum(distinct, b::lit_int(1)))
            }
            RowOperation::Fold { accumulator, initial, body } => {
                // In exact arithmetic a fold a := a +/- term(row) equals the
                // initial value +/- the bag sum. No commutation rule applies to
                // word, IEEE, arbitrary recurrences or partial per-row terms.
                need(rows.fields.contains_key(""), "reduce requires a bound numeric array")?;
                let mut row_locals = locals.clone();
                row_locals.remove(accumulator);
                let start = self.query_value(initial, "", &BTreeMap::new(), env, guard)?;
                need(start.ty() == Ty::Integer, "reduce needs an exact integer initial value")?;
                fn has_accumulator(term: &Scalar, accumulator: &str) -> bool {
                    match term {
                        Scalar::Path(parts) => parts.first().is_some_and(|name| name == accumulator),
                        Scalar::Binary(_, l, r) => has_accumulator(l, accumulator) || has_accumulator(r, accumulator),
                        Scalar::NullPredicate { value, .. } => has_accumulator(value, accumulator),
                        Scalar::Integer(_) | Scalar::Text(_) | Scalar::Bool(_) => false,
                    }
                }
                match body {
                    Scalar::Binary(op @ (Op::Add | Op::Sub), left, term)
                        if matches!(left.as_ref(), Scalar::Path(parts)
                            if parts.as_slice() == std::slice::from_ref(accumulator))
                            && !has_accumulator(term, accumulator) =>
                    {
                        let term = self.query_value(term, &query.alias, &rows.fields, &row_locals, guard)?;
                        need(term.ty() == Ty::Integer, "reduce needs exact integer elements")?;
                        let sum = r::relation_sum(rows.input, term.scalar()?);
                        let value = if *op == Op::Add { b::add(start.scalar()?, sum) } else { b::sub(start.scalar()?, sum) };
                        Value::Term(Ty::Integer, math::normalize(value))
                    }
                    _ => {
                        let RelationExpr::Base { name, .. } = &rows.input else {
                            return Err(fail("pure fold requires a direct ordered source array"));
                        };
                        need(rows.ordering.is_empty(), "pure fold must preserve source array order")?;
                        self.retain_pure_fold(name, accumulator, &query.alias, start.scalar()?, body, guard)?
                    }
                }
            }
            RowOperation::FirstValue { value, fallback } => {
                let mut keys = rows.ordering.into_iter();
                let first_key = keys.next().ok_or_else(|| fail("first requires an ordered input"))?;
                let value = self.query_value(value, &query.alias, &rows.fields, &locals, guard)?;
                let fallback = self.query_value(fallback, "", &BTreeMap::new(), env, guard)?;
                // The declared non-null integer projection makes fallback
                // equivalent to an empty-result default, not a NULL coercion.
                need(value.ty() == Ty::Integer && fallback.ty() == Ty::Integer,
                    "ordered scalar first requires a non-null numeric projection and fallback")?;
                let ordered = r::order_by(rows.input, first_key, keys);
                Value::Term(Ty::Integer, math::normalize(r::relation_first_value(
                    ordered, value.scalar()?, fallback.scalar()?,
                )))
            }
            RowOperation::FirstRow => {
                let mut keys = rows.ordering.into_iter();
                let first_key = keys.next().ok_or_else(|| fail("first requires an ordered input"))?;
                if let Some(declared) = decl {
                    let JavaType::Class(target) = declared else {
                        return Err(fail("first requires a declared row type"));
                    };
                    let class = self
                        .program
                        .find_class(target)
                        .ok_or_else(|| fail("first result has no row declaration"))?;
                    need(
                        class.fields().len() == rows.fields.len()
                            && class.fields().iter().all(|f| {
                                self.project
                                    .parameter_type(f.java_type())
                                    .ok()
                                    .is_some_and(|ty| {
                                        rows.fields.get(f.name()).is_some_and(|v| v.ty() == ty)
                                    })
                            }),
                        "first row does not match declared fields and types",
                    )?;
                }
                let id = self.heap.new_path_object().id();
                self.source_relations.first_rows.insert(id, OrderedRows {
                    input: rows.input,
                    fields: rows.fields,
                    first_key,
                    remaining_keys: keys.collect(),
                });
                env.insert(query.binding.clone(), Value::Term(Ty::Ref, b::heap_ref(id)));
                return Ok(());
            }
            RowOperation::EagerPath(_) | RowOperation::Count { distinct: true } =>
                return Err(fail("incompatible query operation and projection")),
        };
        if let Some(declared) = decl {
            need(
                self.project.parameter_type(declared)? == result.ty(),
                "query result does not match declared local type",
            )?;
        }
        env.insert(query.binding.clone(), result);
        Ok(())
    }

    pub(super) fn row_presence(&self, reference: &Expr) -> Option<Expr> {
        let row = self
            .source_relations
            .first_rows
            .get(&reference.as_heap_ref()?)?;
        Some(r::relation_exists(row.input.clone()))
    }
    pub(super) fn read_selected_field(
        &self,
        reference: &Expr,
        field: &str,
        path: &Expr,
    ) -> Result<Option<Value>> {
        let Some(row) = reference
            .as_heap_ref()
            .and_then(|id| self.source_relations.first_rows.get(&id))
        else {
            return Ok(None);
        };
        let exists = r::relation_exists(row.input.clone());
        need(b::path_implies(path, &exists), "selected row may be absent")?;
        let field = row
            .fields
            .get(field)
            .ok_or_else(|| fail("unknown selected field"))?;
        let ty = field.ty();
        let select = field.scalar()?;
        let declared_text_column = || select.as_row_value().and_then(|value| value.split_once('.'))
            .and_then(|(table, column)| self.source_relations.tables.get(table)?.get(column))
            .is_some_and(|ty| matches!(ty.as_str(), "text" | "text?"));
        // Permit text only when the table declaration also certifies its
        // nullability to the boundary renderer.
        need(ty == Ty::Integer || (ty == Ty::Text && declared_text_column()),
            "first scalar projection requires a number or declared text column")?;
        let ordered = r::order_by(
            row.input.clone(),
            row.first_key.clone(),
            row.remaining_keys.iter().cloned(),
        );
        Ok(Some(Value::Term(ty, r::relation_first_value(ordered, select, b::lit_null()))))
    }
}

pub(super) fn is_scalar_array_relation(relation: &RelationExpr, arrays: &std::collections::BTreeSet<String>) -> bool {
    let mut base = relation;
    loop {
        match base {
            RelationExpr::Distinct(inner) | RelationExpr::Select { input: inner, .. } => base = inner,
            RelationExpr::Base { name, alias } => return alias == "value" && arrays.contains(name),
            _ => return false,
        }
    }
}

pub(super) fn relation_base(relation: &RelationExpr) -> Option<(&str, &str)> {
    let (mut input, _) = r::order_components(relation).unwrap_or((relation, &[]));
    while let RelationExpr::Select { input: child, .. } = input { input = child; }
    if let RelationExpr::Base { name, alias } = input { Some((name, alias)) } else { None }
}

fn join_dependency(predicate: &Expr, left: &str, right: &str) -> u8 {
    let mut mask = 0;
    predicate.walk(&mut |node| {
        if let Some((alias, _)) = node.as_row_value().and_then(|p| p.split_once('.')) {
            if alias == left { mask |= 1; }
            if alias == right { mask |= 2; }
        }
    });
    mask
}

pub(crate) struct InnerJoinCount<'a> {
    pub(crate) left: (&'a str, &'a str),
    pub(crate) right: (&'a str, &'a str),
    pub(crate) on: &'a Expr,
    pub(crate) filters: Vec<&'a Expr>,
}

pub(crate) fn inner_join_count<'a>(outer: &'a RelationExpr, projection: &'a Expr) -> Option<InnerJoinCount<'a>> {
    let RelationExpr::Base { name: left_name, alias: left } = outer else { return None };
    let (math::RelationAggregateKind::Sum, right_relation, Some(one), _) = projection.as_relation_aggregate()? else { return None };
    if one != &b::lit_int(1) { return None; }
    let (right_name, right) = relation_base(right_relation)?;
    if left == right { return None; }
    fn collect<'a>(expr: &'a Expr, out: &mut Vec<&'a Expr>) {
        if let Some((BinOpKind::And, a, b)) = expr.as_binop() {
            collect(a, out);
            collect(b, out);
        } else { out.push(expr); }
    }
    let mut predicates = vec![];
    let mut input = right_relation;
    while let RelationExpr::Select { predicate, input: next } = input {
        collect(predicate, &mut predicates);
        input = next;
    }
    let mut on = None;
    let mut filters = vec![];
    for predicate in predicates {
        if join_dependency(predicate, left, right) == 3 {
            if on.replace(predicate).is_some() { return None; }
        } else { filters.push(predicate); }
    }
    Some(InnerJoinCount { left: (left_name, left), right: (right_name, right), on: on?, filters })
}

pub(crate) fn grouped_table(relation: &RelationExpr) -> Option<(&str, &[Expr])> {
    let RelationExpr::Distinct(projection) = relation else { return None };
    let RelationExpr::Project { value, alias, input } = projection.as_ref() else { return None };
    let RelationExpr::Base { name, alias: table_alias } = input.as_ref() else { return None };
    let keys = value.as_array_lit()?;
    (alias == "groups" && name == table_alias && !keys.is_empty()).then_some((name, keys))
}

pub(super) fn correlated_array_relation(relation: &RelationExpr) -> Option<(&str, &str)> {
    let mut base = relation;
    while let RelationExpr::Select { input, .. } = base { base = input; }
    match base {
        RelationExpr::Vector { value, alias } => Some((value.as_row_value()?, alias)),
        _ => None,
    }
}

pub(crate) fn projected_union_columns(relation: &RelationExpr) -> Option<Vec<(&str, &str)>> {
    let RelationExpr::Union(branches) = relation else { return None; };
    if branches.is_empty() { return None; }
    branches.iter().map(|branch| {
        let RelationExpr::Project { value, alias, input } = branch else { return None; };
        let RelationExpr::Base { name, alias: source } = input.as_ref() else { return None; };
        let (owner, column) = value.as_row_value()?.split_once('.')?;
        (alias == "value" && source == name && owner == name).then_some((name.as_str(), column))
    }).collect()
}

pub(super) fn is_table_expression(
    expr: &Expr,
    tables: &BTreeMap<String, BTreeMap<String, String>>,
    arrays: &std::collections::BTreeSet<String>,
    scalar_element: bool,
    nested_arrays: &std::collections::BTreeSet<String>,
    correlated: &std::collections::BTreeSet<(String, String)>,
    join_tables: &std::collections::BTreeSet<(String, String)>,
    record_inputs: &BTreeMap<String, std::collections::BTreeSet<String>>,
    record_rows: &BTreeMap<String, std::collections::BTreeSet<String>>,
) -> bool {
    if let Some(path) = expr.as_row_value() {
        if (scalar_element && path == "value")
            || correlated.iter().any(|(group, element)| path == group || path == element) {
            return true;
        }
        return path.split_once('.').is_some_and(|(table, name)| {
            tables
                .get(table)
                .is_some_and(|columns| columns.contains_key(name))
                || record_rows.get(table).is_some_and(|fields| fields.contains(name))
        });
    }
    let table_source = |relation: &RelationExpr| relation_base(relation).is_some_and(|(name, alias)| {
        (name == alias && tables.contains_key(name))
            || join_tables.contains(&(name.into(), alias.into()))
            || (alias == "value" && arrays.contains(name))
            || record_inputs.contains_key(name)
    });
    if let Some((kind, relation, value, _)) = expr.as_relation_aggregate() {
        use math::RelationAggregateKind::*;
        return match kind {
            Sum => value.is_some() && (table_source(relation)
                || (value == Some(&b::lit_int(1)) &&
                    grouped_table(relation).is_some_and(|(name, _)| tables.contains_key(name)))
                || relation_base(relation).is_some_and(|(name, _)| nested_arrays.contains(name))
                || correlated_array_relation(relation).is_some_and(|pair|
                    correlated.iter().any(|(group, element)| pair == (group.as_str(), element.as_str())))),
            FirstValue => value.is_some() && table_source(relation),
            Exists => value.is_none_or(|v| v.as_bool() == Some(true)) && table_source(relation),
            Count => (value.is_none() && projected_union_columns(relation).is_some_and(|branches|
                branches.iter().all(|(name, column)| tables.get(*name)
                    .and_then(|columns| columns.get(*column)).is_some_and(|ty| ty == "number"))))
                || is_scalar_array_relation(relation, arrays),
            NthValue => is_scalar_array_relation(relation, arrays),
        };
    }
    if let Some(relation) = expr.as_relation() {
        return table_source(relation);
    }
    if let Some((op, args)) = expr.as_call() {
        if let Some((source, keys)) = args.split_first() {
            return !keys.is_empty()
                && math::func_name(op) == format!("relation.order_by/{}", keys.len())
                && source
                    .as_relation()
                    .is_some_and(|r| table_source(r));
        }
    }
    false
}
