//! Composition of typed local queries with the current Java/action frame.
//! A view binds projected row fields. Later clauses substitute those values
//! into the shared relation algebra, preserving the original table source.
use super::*;
use math::domains::relation::TableSource;
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
    index: Option<i32>,
}

#[derive(Default)]
pub(super) struct Relations {
    pub(super) tables: BTreeMap<String, BTreeMap<String, String>>,
    physically_read: BTreeSet<String>,
    pub(super) record_inputs: BTreeMap<String, String>,
    pub(super) first_rows: BTreeMap<i64, OrderedRows>,
}

/// Here the producer has proved a closed exact-integer sum with the
/// zero default. Admit that sum into AC only with ordinary already-bound
/// exact integer inputs/literals in the initializer; other expressions
/// retain their scope and the conservative generic math behavior.
fn add_closed_integer_sum(initial: Expr, sum: Expr, integer_input: impl Fn(&str) -> bool) -> Expr {
    fn terms(value: &Expr, out: &mut Vec<Expr>, integer_input: &impl Fn(&str) -> bool) -> bool {
        if let Some((BinOpKind::Add, left, right)) = value.as_binop() {
            return terms(left, out, integer_input) && terms(right, out, integer_input);
        }
        let admitted = matches!(value.as_lit(), Some(math::Lit::Int(_) | math::Lit::BigInt(_)))
            || matches!(value.as_sym_var(), Some(SymVarOrigin::Parameter {name, primitive_type})
                if primitive_type == "integer" && integer_input(name));
        if admitted && value != &b::lit_int(0) { out.push(value.clone()); }
        admitted
    }
    let mut addends = vec![sum.clone()];
    if !terms(&initial, &mut addends, &integer_input) { return b::add(initial, sum); }
    addends.sort_by_cached_key(|term| math::canonical_order_key(term, &math::DefaultSortContext));
    addends.into_iter().reduce(b::add).expect("closed sum is present")
}

pub(super) fn scalar_binary(op: Op) -> fn(Expr, Expr) -> Expr {
    match op {
        Op::Add => b::add,
        Op::Sub => b::sub,
        Op::Mul => b::mul,
        Op::Eq => b::eq,
        Op::Ne => b::ne,
        Op::Lt => b::lt,
        Op::Le => b::le,
        Op::Gt => b::gt,
        Op::Ge => b::ge,
    }
}

/// Resolve a row value only in the physical table that owns it.
fn owned_column_domain<'a>(
    columns: &'a BTreeMap<String, String>,
    table: &str,
    (owner, column): (&str, &str),
) -> Option<&'a str> {
    if owner != table {
        return None;
    }
    columns.get(column).map(String::as_str)
}

type TableColumns = BTreeMap<String, String>;

/// Java locals supply a declared shape in addition to the inferred row.
/// Adapter actions have no Java local declaration; their already typed rows
/// carry field names and domains directly.
fn query_row_matches(fields: &BTreeMap<String, Value>, declared: Option<&BTreeMap<String, Ty>>) -> bool {
    declared.is_none_or(|types| types.len() == fields.len()
        && fields.iter().all(|(name, value)| types.get(name) == Some(&value.ty())))
}

/// This is specifically a physical-table proof, not a claim about every
/// finite relation that can be counted.
fn closed_count_table<'a>(
    relation: &'a RelationExpr,
    tables: &'a BTreeMap<String, TableColumns>,
) -> Option<(&'a str, &'a TableColumns, Vec<&'a Expr>)> {
    let mut predicates = Vec::new();
    let mut base = relation;
    while let RelationExpr::Select { predicate, input } = base {
        predicates.push(predicate);
        base = input;
    }
    let RelationExpr::Base { name, alias, .. } = base else { return None; };
    let columns = tables.get(name).filter(|_| name == alias)?;
    Some((name.as_str(), columns, predicates))
}


impl<'a> Engine<'a> {
    /// Only declared base tables filtered by closed, typed expressions in the
    /// current source frame are admitted. An already acquired input field
    /// must resolve through its declared path; raw E/T values stay outside
    /// this proof.
    fn closed_count_source(&self, relation: &RelationExpr) -> bool {
        let Some((name, columns, predicates)) = closed_count_table(relation, &self.source_relations.tables) else {
            return false;
        };
        fn closed(expr: &Expr, table: &str, columns: &TableColumns, entry: &Engine<'_>) -> bool {
            if let Some(literal) = expr.as_lit() {
                return matches!(literal, math::Lit::Int(_) | math::Lit::BigInt(_) | math::Lit::Bool(_) | math::Lit::Str(_) | math::Lit::Null);
            }
            if let Some(row) = expr.as_row_value() {
                return row.split_once('.').and_then(|qualified|
                    owned_column_domain(columns, table, qualified)).is_some();
            }
            if let Some(SymVarOrigin::Parameter { name, primitive_type }) = expr.as_sym_var() {
                return entry.declared_relation_input(name, primitive_type);
            }
            if let Some((op, left, right)) = expr.as_binop() {
                return match op {
                    BinOpKind::And | BinOpKind::Or | BinOpKind::Eq | BinOpKind::Ne
                    | BinOpKind::Lt | BinOpKind::Le | BinOpKind::Gt | BinOpKind::Ge
                    | BinOpKind::Add | BinOpKind::Sub | BinOpKind::Mul =>
                        closed(left, table, columns, entry) && closed(right, table, columns, entry),
                };
            }
            false
        }
        predicates.into_iter().all(|p| closed(p, name, columns, self))
    }

    pub(super) fn declared_relation_input(&self, name: &str, primitive_type: &str) -> bool {
        let mut path = name.split('.');
        let root = path.next().expect("split always yields a root");
        let Some((declared, _)) = self.selected_entry.syntax.params().iter().find(|(_, parameter)| parameter == root)
            else { return false };
        let leaf = path.try_fold(declared, |owner, member| {
            self.project.source_owner(owner).and_then(|owner| self.program.find_class(owner))?
                .fields().iter().find(|field| !field.is_static() && field.name() == member).map(|field| field.java_type())
        });
        leaf.is_some_and(|ty| match primitive_type {
            "integer" => crate::normalized::integral(ty),
            "boolean" => matches!(ty, JavaType::Primitive(name) if name == "boolean"),
            "text" | "text?" => matches!(ty, JavaType::Class(name) if matches!(name.as_str(), "String" | "java.lang.String")),
            _ => false,
        })
    }

    fn query_row_declaration(&self, decl: &JavaType) -> Result<&'a simulator_parser::JavaClass> {
        let component = match decl {
            JavaType::Array(component) => component.as_ref(),
            JavaType::Generic(container, args)
                if matches!(container.as_str(), "List" | "java.util.List") && args.len() == 1 => &args[0],
            _ => return Err(fail(format!(
                "query output requires a declared row array or List<Row>, found {decl:?}"
            ))),
        };
        let JavaType::Class(name) = component else {
            return Err(fail("query output requires a declared row type"));
        };
        self.program.find_class(name).ok_or_else(|| fail("undeclared row type"))
    }

    fn query_row_types(&self, row: &simulator_parser::JavaClass) -> Result<BTreeMap<String, Ty>> {
        row.fields()
            .iter()
            .map(|f| Ok((f.name().into(), self.project.parameter_type(f.java_type())?)))
            .collect()
    }

    fn acquire_table_snapshot(&mut self, table: &str, path: &Expr) -> TableSource {
        use super::realtime_state::{Acquisition, EffectOperation};
        let guard = self.normal_path(path.clone());
        let after: Vec<_> = self.realtime_effects.events().iter().filter_map(|event| {
            let (receipt, success, mutated_table) = match &event.operation {
                EffectOperation::Sql { statement, receipt } => {
                    let success = b::not(math::domains::completion::is_throw(
                        receipt.response().clone(), statement.exception()));
                    (receipt, success, Some(statement.table()))
                }
                EffectOperation::Port { receipt, completion, .. } if completion.throwing() =>
                    (receipt, completion.normal_condition(receipt.response()), None),
                _ => return None,
            };
            let when = math::normalize(b::and(event.guard.clone(), success));
            if math::normalize(b::and(guard.clone(), when.clone())).as_bool() == Some(false) {
                return None;
            }
            let dominant = b::path_implies(&guard, &when);
            // Even an unrelated table can only be read after a failure-
            // gating completion that dominates the read. A conditional
            // data dependency is relevant only to the mutated table.
            if mutated_table != Some(table) && !dominant { return None; }
            let relative = if dominant { b::lit_bool(true) } else { when };
            Some((receipt.binding_id(), relative))
        }).collect();
        let initial = || TableSource { name: table.into(), snapshot: None };
        if guard.as_bool() == Some(false) { return initial(); }
        // A plain table binding can represent the unconditional initial
        // snapshot. It has no occurrence condition: a guarded first read
        // needs an acquisition even if no prior write could affect it.
        // A later physical read is a separate observation in either case.
        let first = self.source_relations.physically_read.insert(table.into());
        if first && after.is_empty() && guard.as_bool() == Some(true) {
            return initial();
        }
        let id = self.fresh_binding();
        self.realtime_effects.acquire(guard, Acquisition::Sql { source: table.into(), after }, id);
        TableSource { name: table.into(), snapshot: Some(id) }
    }

    fn table_rows(
        &mut self,
        query: &Query,
        row: &simulator_parser::JavaClass,
        types: BTreeMap<String, Ty>,
        fields: &[(String, Scalar)],
        env: &Env,
        path: &Expr,
    ) -> Result<Rows> {
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
                        Scalar::Text(_) => Some(Ty::Text),
                        Scalar::Integer(_) => Some(Ty::Integer),
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
        let mut typed_columns = Vec::with_capacity(columns.len());
        for (name, ty) in columns {
            let business_type = super::business_sql::column_type(&ty, nullable_columns.contains(&name))
                .ok_or_else(|| fail("unsupported table column type"))?;
            super::business_sql::register_column(
                table, &query.source, &name, business_type, super::business_sql::ColumnEvidence::Declaration,
            )?;
            typed_columns.push((name, ty, business_type));
        }
        let table = self.acquire_table_snapshot(&query.source, path);
        let input = RelationExpr::Base {
            name: table.name.clone(),
            alias: table.name.clone(),
            snapshot: table.snapshot,
        };
        Ok(Rows {
            input,
            fields: typed_columns
                .into_iter()
                .map(|(name, ty, business_type)| {
                    let value = b::sql_column(&query.source, table.clone(), 0, &name, business_type);
                    (name, Value::Term(ty, value))
                })
                .collect(),
            ordering: vec![],
        })
    }

    // Scalar relation expressions read already-bound values. The enclosing
    // operation owns acquisition paths, exceptions, and effects.
    fn query_value(
        scalar: &Scalar,
        alias: &str,
        fields: &BTreeMap<String, Value>,
        locals: &Env,
    ) -> Result<Value> {
        match scalar {
            Scalar::IntegerBound(_, _) | Scalar::Indexed { .. } => Err(fail("integer bound operations require an integer range")),
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
                        .ok_or_else(|| fail(format!("unknown projected field {}", path.join("."))))?
                } else {
                    if let Some(value) = locals.get(&path.join(".")) {
                        return Ok(value.clone());
                    }
                    locals
                        .get(root)
                        .cloned()
                        .ok_or_else(|| {
                            if locals.keys().any(|slot| slot.strip_prefix(root)
                                .is_some_and(|rest| rest.starts_with('.')))
                            {
                                fail(format!("unknown projected field {}", path.join(".")))
                            } else { fail(format!("undeclared query input {root}")) }
                        })?
                };
                for member in parts {
                    value = values::field(value, member)?;
                }
                Ok(value)
            }
            Scalar::NullPredicate { owner, column, negated } => {
                let value = fields.get(column).filter(|_| owner == alias)
                    .ok_or_else(|| fail("SQL NULL predicate requires a bound row column"))?.scalar()?;
                need(value.as_row_value().is_some(), "SQL NULL predicate requires a table column")?;
                let op = if *negated { b::ne } else { b::eq };
                Ok(Value::Term(Ty::Bool, op(value, b::lit_null())))
            }
            Scalar::Binary(op, left, right) => {
                let left = Self::query_value(left, alias, fields, locals)?;
                let right = Self::query_value(right, alias, fields, locals)?;
                need(left.ty() == right.ty(), "incompatible query operands")?;
                let binary = scalar_binary(*op);
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
        &self, binding: &str, first: &(String, String), remaining: &[(String, String)], env: &mut Env,
    ) -> Result<()> {
        let projection = |(source, field): &(String, String)| -> Result<(r::ProjectedColumn, &'static str)> {
            let Some(Value::Rows(rows)) = env.get(source) else {
                return Err(fail("projected union needs a declared table row source"));
            };
            let mut input = &rows.input;
            let mut filters = Vec::new();
            while let RelationExpr::Select { predicate, input: inner } = input {
                filters.push(predicate.clone());
                input = inner;
            }
            let RelationExpr::Base {name, alias, snapshot} = input else {
                return Err(fail("projected union needs bound table rows"));
            };
            need(rows.ordering.is_empty() && self.source_relations.tables.contains_key(name),
                "projected union requires an unordered read-only table")?;
            let Some(Value::Term(ty, expression)) = rows.fields.get(field) else {
                return Err(fail(format!("projected union requires a declared scalar field: {source}.{field}")));
            };
            let ty = super::business_sql::column_type(ty, false)
                .ok_or_else(|| fail("projected union field needs a supported SQL scalar type"))?;
            let column = expression.as_row_value().and_then(|path| path.split_once('.'))
                .filter(|(root, _)| *root == alias)
                .map(|(_, column)| column)
                .filter(|column| self.source_relations.tables[name].get(*column).is_some_and(|declared| declared == ty))
                .ok_or_else(|| fail("projected union field must be a direct nonnullable table column"))?;
            Ok((r::ProjectedColumn {
                table: TableSource { name: name.clone(), snapshot: *snapshot },
                column: column.into(), filters,
            }, ty))
        };
        let (first, ty) = projection(first)?;
        let remaining = remaining.iter().map(|branch| {
            let (column, declared) = projection(branch)?;
            need(declared == ty, "projected union branches must have the same scalar type")?;
            Ok(column)
        }).collect::<Result<Vec<_>>>()?;
        let count = r::relation_count(RelationExpr::projected_union(first, remaining));
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
        if let Some(Value::Sequence { name, null, .. }) = env.get(&query.source) {
            need(b::path_implies(guard, &math::normalize(b::not(null.clone()))),
                "local relation needs an acquired non-null source array")?;
            need(!self.source_array_may_have_writes(name, guard),
                "use a supported source post-write traversal; local query cannot read a mutated array yet")?;
        }
        if let Operation::OnRows(RowOperation::EagerPath { link, leaf }) = &query.operation {
            need(matches!(decl, Some(JavaType::Array(item)) if crate::normalized::integral(item)),
                "eager projection requires a declared integer array result")?;
            let Some(Value::Sequence { item: Ty::Ref, name, .. }) =
                env.get(&query.source) else { return Err(fail("eager projection requires a directly bound record array")); };
            let root = name.clone();
            let row = self.source_relations.record_inputs.get(&root).and_then(|owner| self.program.find_class(owner))
                .ok_or_else(|| fail("eager projection requires its declared record element"))?;
            let member = row.fields().iter().find(|field| field.name() == link)
                .ok_or_else(|| fail("undeclared eager projection member"))?;
            let nullable = crate::normalized::source_nullable_field(row, member);
            let child = self.project.source_owner(member.java_type()).and_then(|owner| self.program.find_class(owner))
                .ok_or_else(|| fail("eager projection member must be a declared record"))?;
            need(child.fields().iter().any(|field| field.name() == leaf && crate::normalized::integral(field.java_type())),
                "eager projection leaf must be a declared integer")?;
            if nullable {
                let base = RelationExpr::base(&root, "r");
                let missing = b::eq(b::row_value(format!("r.{link}")), b::lit_null());
                // This is the completion of the entire projection, not of a single selected row.
                let exception = r::relation_exists(RelationExpr::select(missing, base));
                self.throw_source_on_path(b::and(guard.clone(), exception), "java.lang.NullPointerException");
            }
            env.insert(query.binding.clone(), Value::EagerIntProjection {
                source: root, field: format!("{link}.{leaf}"),
            });
            return Ok(());
        }
        let operation = match &query.operation {
            Operation::FirstArray { offset, fallback } => {
                let Some(Value::Sequence { item: Ty::Integer, name, .. }) =
                    env.get(&query.source) else { return Err(fail("scalar first needs a bound numeric array")); };
                let relation = RelationExpr::base(name, "value");
                let default = Self::query_value(fallback, "", &BTreeMap::new(), env)?;
                need(default.ty() == Ty::Integer, "scalar first requires a numeric fallback")?;
                need(decl.is_none_or(crate::normalized::integral), "scalar first requires a numeric local")?;
                let index = b::lit_integer(offset.clone());
                let length = r::relation_count(relation.clone());
                let none = if offset.bits() == 0 { b::eq(length, index.clone()) } else { b::le(length, index.clone()) };
                let nth = if offset.bits() == 0 {
                    r::relation_first_value(relation, b::acquired_integer_row("value"), b::lit_null())
                } else { r::relation_nth_value(relation, index) };
                let value = math::normalize(b::case_when(vec![(none, default.scalar()?)], nth));
                env.insert(query.binding.clone(), Value::Term(Ty::Integer, value));
                return Ok(());
            }
            Operation::NestedSum { alias, filters, value } => {
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
                for filter in filters {
                    let predicate = Self::query_value(filter, alias, &row, &inner_locals)?;
                    need(predicate.ty() == Ty::Bool, "inner where requires bool")?;
                    inner_relation = RelationExpr::select(predicate.scalar()?, inner_relation);
                }
                need(query.clauses.is_empty(), "nested sum outer clauses require a group-valued operation")?;
                let element = Self::query_value(value, alias, &row, &inner_locals)?;
                need(element.ty() == Ty::Integer, "sum requires exact integers")?;
                need(decl.is_none_or(crate::normalized::integral), "nested sum requires a numeric local")?;
                let summed = r::relation_sum(outer, r::relation_sum(inner_relation, element.scalar()?));
                let summed = if query.accumulate {
                    let prior = env.get(&query.binding).ok_or_else(|| fail("+= needs an initialized local"))?;
                    need(prior.ty() == Ty::Integer, "+= requires a number")?;
                    add_closed_integer_sum(prior.scalar()?, summed, |name| self.declared_relation_input(name, "integer"))
                } else { summed };
                env.insert(query.binding.clone(), Value::Term(Ty::Integer, math::normalize(summed)));
                return Ok(());
            }
            Operation::JoinCount { source: other, alias: rhs, on, filters } => {
                let mut names = env.clone();
                let mut prefilters = Vec::new();
                let mut bind = |source: &str, alias: &str, row_slot| -> Result<(TableSource, String)> {
                    need(!env.contains_key(alias), "join alias would shadow a local value")?;
                    let Some(Value::Rows(rows)) = env.get(source) else {
                        return Err(fail("join requires two typed table bindings"));
                    };
                    let mut input = &rows.input;
                    let mut source_filters = Vec::new();
                    while let RelationExpr::Select { predicate, input: inner } = input {
                        source_filters.push(predicate);
                        input = inner;
                    }
                    let RelationExpr::Base { name, alias: bound, snapshot } = input else {
                        return Err(fail("join count requires a bound table source"));
                    };
                    need(rows.ordering.is_empty(), "join count requires unordered input rows")?;
                    let source_table = TableSource { name: name.clone(), snapshot: *snapshot };
                    for filter in source_filters {
                        prefilters.push(r::rebind_join_predicate(filter, &source_table, row_slot, alias)
                            .ok_or_else(|| fail("join source filter must be a closed predicate over that input"))?);
                    }
                    let columns = self.source_relations.tables.get(name).filter(|_| name == bound)
                        .ok_or_else(|| fail("join count requires a declared table"))?;
                    for (field, value) in &rows.fields {
                        let ty = value.ty();
                        let value = value.scalar()?;
                        let (table, column) = value.as_row_value().and_then(|p| p.split_once('.'))
                            .ok_or_else(|| fail("join requires direct table columns"))?;
                        let domain = owned_column_domain(columns, name, (table, column))
                            .filter(|_| field == column)
                            .ok_or_else(|| fail("join table declaration must keep physical column names"))?;
                        need(domain != "text?",
                            "join count currently requires non-nullable columns")?;
                        let declared = super::business_sql::column_type(&ty, false)
                            .filter(|declared| *declared == domain)
                            .ok_or_else(|| fail("join column type differs from its physical declaration"))?;
                        let table = TableSource { name: name.clone(), snapshot: *snapshot };
                        names.insert(format!("{alias}.{field}"),
                            Value::Term(ty, b::sql_column(alias, table, row_slot, field, declared)));
                    }
                    Ok((TableSource { name: name.clone(), snapshot: *snapshot }, alias.into()))
                };
                let left = bind(&query.source, &query.alias, 0)?;
                let right = bind(other, rhs, 1)?;
                let predicate = |expr| {
                    let value = Self::query_value(expr, "", &BTreeMap::new(), &names)?;
                    need(value.ty() == Ty::Bool, "join predicate must be bool")?;
                    value.scalar()
                };
                let on = predicate(on)?;
                let dependency = |predicate: &Expr| join_dependency(predicate, &query.alias, rhs);
                need(dependency(&on) == 3, "join ON must depend on both typed rows")?;
                let mut filters = filters.iter().map(|clause| {
                    let filter = predicate(clause)?;
                    need(dependency(&filter) != 3, "join WHERE may filter one row per clause")?;
                    Ok(filter)
                }).collect::<Result<Vec<_>>>()?;
                // These are inner-join selections over already acquired
                // rows. Apply the source restrictions in WHERE, never ON.
                filters.extend(prefilters);
                need(decl.is_none_or(crate::normalized::integral), "join count needs a numeric local")?;
                // SUM(1) over the matched pairs preserves their multiplicity.
                let result = math::normalize(r::relation_sum(RelationExpr::Join { left, right, on, filters }, b::lit_int(1)));
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
                    let name = format!("{}.{}", query.alias, field.name());
                    let value = if ty == Ty::Integer { b::acquired_integer_row(name) } else { b::row_value(name) };
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
                        let path = format!("{}.{}", query.alias, name);
                        let field = if ty == Ty::Integer { b::acquired_integer_row(path) } else { b::row_value(path) };
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
            None => {
                let RowOperation::Project(fields) = operation else {
                    return Err(fail("aggregate source is not bound"));
                };
                let row = self.query_row_declaration(
                    decl.ok_or_else(|| fail("table query needs a row declaration"))?,
                )?;
                let types = self.query_row_types(row)?;
                self.table_rows(query, row, types, fields, env, guard)?
            }
        };
        for clause in &query.clauses {
            match clause {
                Clause::Where(expr) => {
                    let val = Self::query_value(expr, &query.alias, &rows.fields, &locals)?;
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
                        Self::query_value(expr, &query.alias, &rows.fields, &locals)?,
                    );
                }
                Clause::Order(keys) => {
                    need(rows.ordering.is_empty(), "multiple order clauses")?;
                    rows.ordering = keys
                        .iter()
                        .map(|key| {
                            let value =
                                Self::query_value(key, &query.alias, &rows.fields, &locals)?;
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
                            Self::query_value(e, &query.alias, &rows.fields, &locals)?,
                        ))
                    })
                    .collect::<Result<_>>()?;
                if let Some(declared) = decl {
                    let types = self.query_row_types(self.query_row_declaration(declared)?)?;
                    need(query_row_matches(&rows.fields, Some(&types)),
                        "row projection does not match declared fields and types")?;
                }
                env.insert(query.binding.clone(), Value::Rows(Box::new(rows)));
                return Ok(());
            }
            RowOperation::Sum(expr) => {
                let value = Self::query_value(expr, &query.alias, &rows.fields, &locals)?;
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
            RowOperation::CountProjection { distinct, value } => {
                let value = Self::query_value(value, &query.alias, &rows.fields, &locals)?;
                need(matches!(value.ty(), Ty::Integer | Ty::Text | Ty::Bool),
                    "projected count requires a declared scalar value")?;
                let value = value.scalar()?;
                let count = if *distinct {
                    r::relation_count_distinct(rows.input, value)
                } else {
                    r::relation_count_nonnull(rows.input, value)
                };
                Value::Term(Ty::Integer, math::normalize(count))
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
            RowOperation::GroupCount { first, remaining } => {
                // Each group is one distinct *tuple* of evaluated keys, not
                // independent distinct counts and not the source row count.
                let RelationExpr::Base { name, alias, snapshot } = &rows.input else {
                    return Err(fail("group count currently requires an unfiltered table declaration"));
                };
                let columns = self.source_relations.tables.get(name).filter(|_| name == alias)
                    .ok_or_else(|| fail("group count requires a declared table"))?;
                let physical_column = |key| {
                    let value = Self::query_value(key, &query.alias, &rows.fields, &locals)?.scalar()?;
                    let (_, column) = value.as_row_value().and_then(|path| path.split_once('.'))
                        .filter(|qualified| owned_column_domain(columns, name, *qualified).is_some())
                        .ok_or_else(|| fail("group key requires a declared table column"))?;
                    Ok(column.to_owned())
                };
                let first = physical_column(first)?;
                let remaining = remaining.iter().map(physical_column).collect::<Result<Vec<_>>>()?;
                let grouped = RelationExpr::grouped(TableSource { name: name.clone(), snapshot: *snapshot }, first, remaining);
                Value::Term(Ty::Integer, r::relation_sum(grouped, b::lit_int(1)))
            }
            RowOperation::FirstValue { value, fallback } => {
                let mut keys = rows.ordering.into_iter();
                let first_key = keys.next().ok_or_else(|| fail("first requires an ordered input"))?;
                let value = Self::query_value(value, &query.alias, &rows.fields, &locals)?;
                let fallback = Self::query_value(fallback, "", &BTreeMap::new(), env)?;
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
                let declared = match decl {
                    None => None,
                    Some(JavaType::Class(target)) => {
                        let class = self.program.find_class(target)
                            .ok_or_else(|| fail("first result has no row declaration"))?;
                        let types = self.query_row_types(class)
                            .map_err(|_| fail("first row does not match declared fields and types"))?;
                        Some(types)
                    }
                    Some(_) => return Err(fail("first requires a declared row type")),
                };
                need(query_row_matches(&rows.fields, declared.as_ref()),
                    "first row does not match declared fields and types")?;
                let id = self.heap.new_path_object().id();
                self.source_relations.first_rows.insert(id, OrderedRows {
                    input: rows.input,
                    fields: rows.fields,
                    first_key,
                    remaining_keys: keys.collect(),
                    index: None,
                });
                env.insert(query.binding.clone(), Value::Term(Ty::Ref, b::heap_ref(id)));
                return Ok(());
            }
            RowOperation::EagerPath { .. } | RowOperation::Count { distinct: true } =>
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

    /// A non-null literal fallback cannot satisfy the null test. Under the
    /// presence guard the ordered first value is unaffected by its fallback.
    pub(super) fn compare_first_text_to_null(&self, expression: &Expr, different: bool) -> Option<Expr> {
        let (math::RelationAggregateKind::FirstValue, ordered, Some(value), Some(fallback)) =
            expression.as_relation_aggregate()? else { return None; };
        if !matches!(fallback.as_lit(), Some(math::Lit::Str(_))) {
            return None;
        }
        let (rows, _) = r::order_components(ordered)?;
        let (owner, column) = value.as_row_value()?.split_once('.')?;
        match self.source_relations.tables.get(owner)?.get(column)?.as_str() {
            // A present row supplies declared non-null text; absence uses
            // the non-null fallback already proved above.
            "text" => return Some(b::lit_bool(different)),
            "text?" => {}
            _ => return None,
        }
        let present = r::relation_exists(rows.clone());
        let first = r::relation_first_value(ordered.clone(), value.clone(), b::lit_null());
        let test = if different { b::ne(first, b::lit_null()) } else { b::eq(first, b::lit_null()) };
        Some(b::case_when(vec![(present, test)], b::lit_bool(different)))
    }

    pub(super) fn index_ordered_table_array(
        &mut self, rows: Rows, index: i32, path: Expr,
    ) -> Result<Value> {
        let mut keys = rows.ordering.into_iter();
        let first_key = keys.next().ok_or_else(|| fail("indexed table array requires declared order"))?;
        let (name, alias) = relation_base(&rows.input).ok_or_else(|| fail("expected acquired table rows"))?;
        need(name == alias && self.source_relations.tables.contains_key(name),
            "indexed table array requires an acquired physical table")?;
        let outside = if index < 0 {
            b::lit_bool(true)
        } else if index == 0 {
            b::not(r::relation_exists(rows.input.clone()))
        } else {
            let length = r::relation_sum(rows.input.clone(), b::lit_int(1));
            b::le(length, b::lit_int(i64::from(index)))
        };
        self.throw_source_on_path(b::and(path, outside), "java.lang.ArrayIndexOutOfBoundsException");
        let id = self.heap.new_path_object().id();
        self.source_relations.first_rows.insert(id, OrderedRows {
            input: rows.input, fields: rows.fields, first_key,
            remaining_keys: keys.collect(), index: Some(index.max(0)),
        });
        Ok(Value::Term(Ty::Ref, b::heap_ref(id)))
    }

    pub(super) fn row_presence(&self, reference: &Expr) -> Option<Expr> {
        let row = self
            .source_relations
            .first_rows
            .get(&reference.as_heap_ref()?)?;
        Some(if row.index.is_some() { b::lit_bool(true) } else { r::relation_exists(row.input.clone()) })
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
        if row.index.is_none() {
            need(b::path_implies(path, &exists), "selected row may be absent")?;
        }
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
        let selected = match row.index {
            None | Some(0) => r::relation_first_value(ordered, select, b::lit_null()),
            Some(index) => r::relation_nth_value(
                RelationExpr::project(select, "value", ordered), b::lit_int(i64::from(index)),
            ),
        };
        Ok(Some(Value::Term(ty, selected)))
    }
}

pub(super) fn is_scalar_array_relation(relation: &RelationExpr, arrays: &std::collections::BTreeSet<String>) -> bool {
    let mut base = relation;
    loop {
        match base {
            RelationExpr::Distinct(inner) | RelationExpr::Select { input: inner, .. } => base = inner,
            RelationExpr::Base { name, alias, snapshot: None } => return alias == "value" && arrays.contains(name),
            _ => return false,
        }
    }
}

pub(super) fn relation_base(relation: &RelationExpr) -> Option<(&str, &str)> {
    let (mut input, _) = r::order_components(relation).unwrap_or((relation, &[]));
    while let RelationExpr::Select { input: child, .. } = input { input = child; }
    if let RelationExpr::Base { name, alias, .. } = input { Some((name, alias)) } else { None }
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
            || (alias == "value" && arrays.contains(name))
            || record_inputs.contains_key(name)
    });
    if let Some((kind, relation, value, _)) = expr.as_relation_aggregate() {
        use math::RelationAggregateKind::*;
        return match kind {
            // The aggregate constructors require a projection for these kinds.
            Sum => table_source(relation)
                || (value == Some(&b::lit_int(1)) &&
                    matches!(relation, RelationExpr::Grouped(group) if tables.get(&group.table().name)
                        .is_some_and(|columns| group.columns().all(|column| columns.contains_key(column)))))
                || (value == Some(&b::lit_int(1)) &&
                    matches!(relation, RelationExpr::Join { left, right, .. }
                        if tables.contains_key(&left.0.name) && tables.contains_key(&right.0.name)))
                || relation_base(relation).is_some_and(|(name, _)| nested_arrays.contains(name))
                || correlated_array_relation(relation).is_some_and(|pair|
                    correlated.iter().any(|(group, element)| pair == (group.as_str(), element.as_str()))),
            FirstValue => table_source(relation),
            Exists => table_source(relation),
            Count => (value.is_some() && table_source(match relation {
                    RelationExpr::Distinct(input) => input, other => other,
                }))
                || (value.is_none() && matches!(relation, RelationExpr::ProjectedUnion(branches) if {
                    let mut domains = branches.iter().map(|branch| {
                        tables.get(&branch.table.name).and_then(|columns| columns.get(&branch.column)).map(String::as_str)
                    });
                    domains.next().flatten().is_some_and(|first|
                        matches!(first, "number" | "text") && domains.all(|domain| domain == Some(first)))
                }))
                || is_scalar_array_relation(relation, arrays)
                || matches!(relation, RelationExpr::Base { name, .. } if record_inputs.contains_key(name)),
            NthValue => matches!(relation, RelationExpr::Project { input, alias, .. }
                    if alias == "value" && table_source(input))
                || is_scalar_array_relation(relation, arrays)
                || matches!(relation, RelationExpr::RecordProjection { source, .. } if record_inputs.contains_key(source)),
        };
    }
    if let Some(relation) = expr.as_relation() {
        return table_source(relation);
    }
    r::order_value_components(expr).is_some_and(|(source, _)| table_source(source))
}
