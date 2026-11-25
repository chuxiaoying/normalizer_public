//! Composition of typed local queries with the current Java/action frame.
//! A view binds projected row fields. Later clauses substitute those values
//! into the shared relation algebra, preserving the original table source.
use super::*;
use normalizer_ir::local::{
    relation::{Clause, Kind, Op, Projection, Query, Scalar},
    ScalarOperand,
};

#[derive(Clone, PartialEq)]
pub(super) struct Rows {
    input: RelationExpr,
    fields: BTreeMap<String, Value>,
    ordering: Vec<Expr>,
}
impl Rows {
    pub(super) fn ty(&self) -> Ty {
        Ty::Vector(Box::new(Ty::Record(
            self.fields
                .iter()
                .map(|(k, v)| (k.clone(), v.ty()))
                .collect(),
        )))
    }
}
#[derive(Default)]
pub(super) struct Relations {
    pub(super) tables: BTreeMap<String, BTreeMap<String, String>>,
    pub(super) record_inputs: BTreeMap<String, String>,
    pub(super) first_rows: BTreeMap<i64, Rows>,
}

impl Engine<'_> {
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

    fn query_row_type(&self, decl: &JavaType) -> Result<BTreeMap<String, Ty>> {
        self.query_row_declaration(decl)?.fields()
            .iter()
            .map(|f| Ok((f.name().into(), self.project.parameter_type(f.java_type())?)))
            .collect()
    }

    fn table_rows(&mut self, query: &Query, decl: &JavaType, env: &Env) -> Result<Rows> {
        let types = self.query_row_type(decl)?;
        let Projection::Fields(fields) = &query.projection else {
            return Err(fail("table rows require a record projection"));
        };
        let row = self.query_row_declaration(decl)?;
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
                        Scalar::Literal(ScalarOperand::Integer(_)) => Some(Ty::Integer),
                        Scalar::Literal(ScalarOperand::Text(_)) => Some(Ty::Text),
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
            Scalar::Literal(ScalarOperand::Text(text)) => Ok(Value::Term(Ty::Text, b::lit_str(text))),
            Scalar::Literal(ScalarOperand::Integer(n)) => {
                Ok(Value::Term(Ty::Integer, b::lit_integer(n.clone())))
            }
            Scalar::Literal(ScalarOperand::Local(_)) => Err(fail("unexpected local literal")),
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
                let (mut a, mut b) = (left.scalar()?, right.scalar()?);
                if t == Ty::Bool && left.ty() == Ty::Integer {
                    // An exact difference compared with zero may compare the operands
                    // directly. Both sides still require the same row and port values.
                    if b == b::lit_int(0) {
                        if let Some((BinOpKind::Sub, x, y)) = a.as_binop() {
                            (a, b) = (x.clone(), y.clone());
                        }
                    } else if a == b::lit_int(0) {
                        if let Some((BinOpKind::Sub, x, y)) = b.as_binop() {
                            (a, b) = (y.clone(), x.clone());
                        }
                    }
                }
                Ok(Value::Term(t, math::normalize(binary(a, b))))
            }
        }
    }

    pub(super) fn execute_query(
        &mut self,
        query: &Query,
        decl: Option<&JavaType>,
        env: &mut Env,
        guard: &Expr,
    ) -> Result<()> {
        if let Some(Value::Sequence { relation: RelationExpr::Base { name, .. }, .. }) = env.get(&query.source) {
            need(!self.source_array_writes.contains_key(name),
                "use a supported source post-write traversal; local query cannot read a mutated array yet")?;
        }
        if let Kind::FirstScalar(offset) = query.kind {
            let Some(Value::Sequence { item: Ty::Integer, relation: RelationExpr::Base { name, .. }, .. }) =
                env.get(&query.source) else { return Err(fail("scalar first needs a bound numeric array")); };
            let relation = RelationExpr::base(name, "value");
            let Projection::Value(default) = &query.projection else { return Err(fail("scalar first requires a fallback value")); };
            let default = self.query_value(default, "", &BTreeMap::new(), env, guard)?;
            need(default.ty() == Ty::Integer, "scalar first requires a numeric fallback")?;
            need(decl.is_none_or(crate::normalized::integral), "scalar first requires a numeric local")?;
            let index = b::lit_int(offset as i64);
            let length = r::relation_count(relation.clone());
            let none = if offset == 0 { b::eq(length, index.clone()) } else { b::le(length, index.clone()) };
            let nth = r::relation_nth_value(relation, index, b::lit_null());
            let value = b::case_when(vec![(none, default.scalar()?), (b::lit_bool(true), nth)]);
            env.insert(query.binding.clone(), Value::Term(Ty::Integer, value));
            return Ok(());
        }
        if let Projection::NestedSum(inner) = &query.projection {
            let Some(Value::Sequence { item: Ty::Seq(element), relation: RelationExpr::Base { name, .. }, .. }) = env.get(&query.source) else {
                return Err(fail("nested sum requires a directly bound two-dimensional array"));
            };
            need(**element == Ty::Integer && query.kind == Kind::Sum && inner.kind == Kind::Sum,
                "nested sum requires exact integer elements")?;
            let Projection::Value(element) = &inner.projection else {
                return Err(fail("nested sum requires a scalar projection"));
            };
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
            for clause in &inner.clauses {
                let Clause::Where(p) = clause else {
                    return Err(fail("nested sum supports scalar filters only"));
                };
                let predicate = self.query_value(p, &inner.alias, &row, &inner_locals, guard)?;
                need(predicate.ty() == Ty::Bool, "inner where requires bool")?;
                inner_relation = RelationExpr::select(predicate.scalar()?, inner_relation);
            }
            need(query.clauses.is_empty(), "nested sum outer clauses require a group-valued operation")?;
            let element = self.query_value(element, &inner.alias, &row, &inner_locals, guard)?;
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
        if let Projection::JoinCount { source: other, alias: rhs, on, filters } = &query.projection {
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
        let mut locals = env.clone();
        let mut rows = match env.get(&query.source) {
            Some(Value::Rows(rows)) => (**rows).clone(),
            Some(Value::Sequence { item: Ty::Integer, relation, .. }) => {
                let RelationExpr::Base { name, .. } = relation else {
                    return Err(fail("expected a directly bound numeric sequence"));
                };
                Rows {
                    input: RelationExpr::base(name, "value"),
                    fields: BTreeMap::from([(String::new(), Value::Term(Ty::Integer, b::row_value("value")))]),
                    ordering: vec![],
                }
            }
            Some(Value::Sequence { item: Ty::Ref, relation: RelationExpr::Base { name, .. }, .. }) => {
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
            None if query.kind == Kind::Rows => self.table_rows(
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
        let result = match (&query.kind, &query.projection) {
            (Kind::Rows, Projection::Fields(fields)) => {
                rows.fields = fields
                    .iter()
                    .map(|(k, e)| {
                        Ok((
                            k.clone(),
                            self.query_value(e, &query.alias, &rows.fields, &locals, guard)?,
                        ))
                    })
                    .collect::<Result<_>>()?;
                Value::Rows(Box::new(rows))
            }
            (Kind::Sum, Projection::Value(expr)) => {
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
            (Kind::Count | Kind::CountDistinct, Projection::Value(Scalar::Path(parts)))
                if rows.fields.contains_key("") && parts.as_slice() == std::slice::from_ref(&query.alias) =>
            {
                let row = rows.fields[""].scalar()?;
                let count = if query.kind == Kind::CountDistinct {
                    r::relation_count_distinct(rows.input, row)
                } else {
                    r::relation_count_nonnull(rows.input, row)
                };
                Value::Term(Ty::Integer, math::normalize(count))
            }
            (Kind::Count, Projection::Value(Scalar::Path(parts)))
                if parts.as_slice() == std::slice::from_ref(&query.alias) =>
            {
                // Count rows, irrespective of which of their columns are NULL.
                Value::Term(Ty::Integer, math::normalize(r::relation_sum(rows.input, b::lit_int(1))))
            }
            (Kind::GroupCount, Projection::GroupKeys(keys)) => {
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
            (Kind::Reduce, Projection::Fold { accumulator, initial, body }) => {
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
                        Scalar::Literal(_) | Scalar::Bool(_) => false,
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
            (Kind::First, Projection::FirstValue { value, fallback }) => {
                need(!rows.ordering.is_empty(), "first requires an ordered input")?;
                let value = self.query_value(value, &query.alias, &rows.fields, &locals, guard)?;
                let fallback = self.query_value(fallback, "", &BTreeMap::new(), env, guard)?;
                // The declared non-null integer projection makes fallback
                // equivalent to an empty-result default, not a NULL coercion.
                need(value.ty() == Ty::Integer && fallback.ty() == Ty::Integer,
                    "ordered scalar first requires a non-null numeric projection and fallback")?;
                let ordered = r::order_by(rows.input, rows.ordering);
                Value::Term(Ty::Integer, math::normalize(r::relation_first_value(
                    ordered, value.scalar()?, fallback.scalar()?,
                )))
            }
            (Kind::First, Projection::Value(Scalar::Path(path))) if matches!(path.as_slice(),[s] if s==&query.alias) =>
            {
                need(!rows.ordering.is_empty(), "first requires an ordered input")?;
                let id = self
                    .heap
                    .new_path_object()
                    .id();
                self.source_relations.first_rows.insert(id, rows);
                Value::Term(Ty::Ref, b::heap_ref(id))
            }
            _ => return Err(fail("incompatible query operation and projection")),
        };
        if let Some(declared) = decl {
            match &result {
                Value::Rows(rows) => {
                    let types = self.query_row_type(declared)?;
                    need(
                        types.len() == rows.fields.len()
                            && rows
                                .fields
                                .iter()
                                .all(|(name, value)| types.get(name) == Some(&value.ty())),
                        "row projection does not match declared fields and types",
                    )?;
                }
                Value::Term(Ty::Ref, reference) => {
                    let first = reference
                        .as_heap_ref()
                        .and_then(|id| self.source_relations.first_rows.get(&id))
                        .ok_or_else(|| fail("expected selected row reference"))?;
                    let JavaType::Class(target) = declared else {
                        return Err(fail("first requires a declared row type"));
                    };
                    let class = self
                        .program
                        .find_class(target)
                        .ok_or_else(|| fail("first result has no row declaration"))?;
                    need(
                        class.fields().len() == first.fields.len()
                            && class.fields().iter().all(|f| {
                                self.project
                                    .parameter_type(f.java_type())
                                    .ok()
                                    .is_some_and(|ty| {
                                        first.fields.get(f.name()).is_some_and(|v| v.ty() == ty)
                                    })
                            }),
                        "first row does not match declared fields and types",
                    )?;
                }
                _ => need(
                    self.project.parameter_type(declared)? == result.ty(),
                    "query result does not match declared local type",
                )?,
            }
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
        need(
            field.ty() == Ty::Integer,
            "first scalar projection requires a number",
        )?;
        let ordered = r::order_by(row.input.clone(), row.ordering.clone());
        Ok(Some(Value::Term(
            Ty::Integer,
            r::relation_first_value(ordered, field.scalar()?, b::lit_null()),
        )))
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
        match kind {
            Sum if value.is_some() => return table_source(relation)
                || (value == Some(&b::lit_int(1)) &&
                    grouped_table(relation).is_some_and(|(name, _)| tables.contains_key(name)))
                || relation_base(relation).is_some_and(|(name, _)| nested_arrays.contains(name))
                || correlated_array_relation(relation).is_some_and(|pair|
                    correlated.iter().any(|(group, element)| pair == (group.as_str(), element.as_str()))),
            FirstValue if value.is_some() => return table_source(relation),
            Exists if value.is_none_or(|v| v.as_bool() == Some(true)) => return table_source(relation),
            Count | NthValue => return is_scalar_array_relation(relation, arrays),
            _ => {}
        }
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
