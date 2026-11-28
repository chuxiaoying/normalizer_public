//! Declarative finite integer domains. Bounds are evaluated in the enclosing
//! scope, and the index exists only in the relation's predicates and projection.
use super::*;
use math::domains::relation::positional::{count, integer_polynomial, interval_sum, sum_polynomial};
use normalizer_ir::local::relation::{IntegerBound, IntegerRange, Op, RangeOperation, RangeScope, Scalar};

fn integer(value: Value) -> Result<Expr> {
    need(value.ty() == Ty::Integer, "integer range requires an exact integer")?;
    value.scalar()
}

fn expression(scalar: &Scalar, env: &Env, position: Option<&Position<'_, '_>>) -> Result<Value> {
    let (ty, value) = match scalar {
        Scalar::Integer(n) => (Ty::Integer, b::lit_integer(n.clone())),
        Scalar::Bool(value) => (Ty::Bool, b::lit_bool(*value)),
        Scalar::Text(value) => (Ty::Text, b::lit_str(value)),
        Scalar::Path(names) => {
            if let Some(value) = env.get(&names.join(".")) { return Ok(value.clone()); }
            let (root, rest) = names.split_first().ok_or_else(|| fail("empty range operand"))?;
            let value = env.get(root).cloned().ok_or_else(|| fail(format!("unknown integer range operand {root}")))?;
            return rest.iter().try_fold(value, |owner, field| values::field(owner, field));
        }
        Scalar::Indexed { source, index, fields } => return position
            .ok_or_else(|| fail("index projection requires a proved integer relation domain"))?
            .read(source, index, fields, env),
        Scalar::IntegerBound(IntegerBound::Length, args) => {
            let value = expression(&args[0], env, position)?;
            let length = match value {
                Value::Sequence { name, null, .. } if null.as_bool() == Some(false) =>
                    super::source_arrays::borrowed_array_length(&name),
                Value::LocalIntArray(array) => array.length_expr(),
                _ => return Err(fail("length requires an acquired non-null sequence")),
            };
            (Ty::Integer, length)
        }
        Scalar::IntegerBound(bound, args) => {
            let left = integer(expression(&args[0], env, position)?)?;
            let right = integer(expression(&args[1], env, position)?)?;
            if *bound == IntegerBound::Min && is_sequence_length(&left) && is_sequence_length(&right) {
                return Ok(Value::Term(Ty::Integer, math::domains::relation::positional::minimum(left, right)));
            }
            let cmp = if *bound == IntegerBound::Min { b::lt } else { b::gt };
            (Ty::Integer, b::case_when(vec![(cmp(left.clone(), right.clone()), left)], right))
        }
        Scalar::Binary(op, lhs, rhs) => {
            let left = expression(lhs, env, position)?;
            let right = expression(rhs, env, position)?;
            need(left.ty() == right.ty(), "range operands have different domains")?;
            if !matches!(op, Op::Eq | Op::Ne) {
                need(left.ty() == Ty::Integer, "range arithmetic or order requires integers")?;
            }
            let (ty, make): (Ty, fn(Expr, Expr) -> Expr) = match op {
                Op::Add => (Ty::Integer, b::add),
                Op::Sub => (Ty::Integer, b::sub),
                Op::Mul => (Ty::Integer, b::mul),
                Op::Eq => (Ty::Bool, b::eq),
                Op::Ne => (Ty::Bool, b::ne),
                Op::Lt => (Ty::Bool, b::lt),
                Op::Le => (Ty::Bool, b::le),
                Op::Gt => (Ty::Bool, b::gt),
                Op::Ge => (Ty::Bool, b::ge),
            };
            (ty, make(left.scalar()?, right.scalar()?))
        }
        Scalar::NullPredicate { .. } => return Err(fail("SQL NULL test does not belong to an integer range")),
    };
    Ok(Value::Term(ty, math::normalize(value)))
}

fn is_sequence_length(value: &Expr) -> bool {
    matches!(value.as_relation_aggregate(), Some((math::RelationAggregateKind::Count, RelationExpr::Base { .. }, None, None)))
}

struct Position<'a, 'p> {
    engine: &'a Engine<'p>,
    alias: &'a str,
    lower: &'a Expr,
    upper: &'a Expr,
    path: &'a Expr,
}

impl Position<'_, '_> {
    fn read(&self, source: &str, index: &Scalar, fields: &[String], env: &Env) -> Result<Value> {
        let Some(Value::Sequence { item, name, null, .. }) = env.get(source) else {
            return Err(fail(format!("index projection needs an acquired sequence: {source}")));
        };
        let length = super::source_arrays::borrowed_array_length(name);
        let mut endpoints = env.clone();
        let substitute = |env: &mut Env, index: Expr| {
            env.insert(self.alias.into(), Value::Term(Ty::Integer, index));
        };
        substitute(&mut endpoints, self.lower.clone());
        let first = integer(expression(index, &endpoints, None)?)?;
        substitute(&mut endpoints, math::normalize(b::sub(self.upper.clone(), b::lit_int(1))));
        let last = integer(expression(index, &endpoints, None)?)?;
        let coefficient = slope(index, self.alias).ok_or_else(|| fail("indexed projection needs a provable affine subscript"))?;
        let (low, high) = if coefficient < 0 { (last, first) } else { (first, last) };
        let nonempty = math::normalize(b::and(self.path.clone(), b::lt(self.lower.clone(), self.upper.clone())));
        let high_exclusive = math::normalize(b::add(high, b::lit_int(1)));
        let mut probe = env.clone();
        let row = b::row_value("$position_proof");
        substitute(&mut probe, row.clone());
        let subscript = integer(expression(index, &probe, None)?)?;
        let reverse = math::normalize(b::sub(b::sub(length.clone(), b::lit_int(1)), row.clone()));
        let direct_or_reversed_from_zero = self.lower.as_lit().and_then(math::Lit::as_integer).is_some_and(|n| n == 0.into())
            && (subscript == row || subscript == reverse);
        need(b::path_implies(&nonempty, &math::normalize(b::not(null.clone()))),
            "index projection cannot prove a non-null sequence")?;
        need(!self.engine.source_array_may_have_writes(name, &nonempty),
            "index projection requires an unchanged acquired sequence")?;
        need((direct_or_reversed_from_zero && upper_bounded_by(self.upper, &length, self.path))
            || b::path_implies(&nonempty, &math::normalize(b::ge(low, b::lit_int(0))))
                && upper_bounded_by(&high_exclusive, &length, &nonempty),
            "index projection cannot prove its subscript stays within the sequence")?;
        let index = integer(expression(index, env, None)?)?;
        if fields.is_empty() && *item == Ty::Integer {
            return Ok(Value::Term(Ty::Integer, r::relation_nth_value(RelationExpr::base(name, "value"), index)));
        }
        let [field] = fields else { return Err(fail("index projection needs a declared scalar field")); };
        let owner = self.engine.source_relations.record_inputs.get(name)
            .and_then(|name| self.engine.program.find_class(name))
            .ok_or_else(|| fail("indexed field needs an acquired record sequence"))?;
        let declared = owner.fields().iter().find(|candidate| !candidate.is_static() && candidate.name() == field)
            .ok_or_else(|| fail(format!("undeclared indexed field {field}")))?;
        let (ty, primitive) = if crate::normalized::integral(declared.java_type()) {
            (Ty::Integer, math::Primitive::Number)
        } else if matches!(declared.java_type(), JavaType::Primitive(name) if name == "boolean") {
            (Ty::Bool, math::Primitive::Boolean)
        } else { return Err(fail("indexed field needs an exact integer or Boolean domain")); };
        Ok(Value::Term(ty, math::domains::relation::positional::field_at(name, index, field, primitive)))
    }
}

fn slope(expr: &Scalar, alias: &str) -> Option<i64> {
    match expr {
        Scalar::Path(path) if path.len() == 1 && path[0] == alias => Some(1),
        Scalar::Path(_) | Scalar::Integer(_) => Some(0),
        Scalar::Binary(Op::Add, a, b) => slope(a, alias)?.checked_add(slope(b, alias)?),
        Scalar::Binary(Op::Sub, a, b) => slope(a, alias)?.checked_sub(slope(b, alias)?),
        Scalar::Binary(Op::Mul, a, b) if slope(a, alias)? == 0 && slope(b, alias)? == 0 => Some(0),
        _ => None,
    }
}

fn upper_bounded_by(value: &Expr, limit: &Expr, active: &Expr) -> bool {
    use math::domains::relation::positional::{integer_domain, minimum_bounds, range_bounds};
    if value == limit || b::path_implies(active, &math::normalize(b::le(value.clone(), limit.clone()))) {
        return true;
    }
    if let Some((a, b)) = minimum_bounds(value) {
        return upper_bounded_by(a, limit, active) || upper_bounded_by(b, limit, active);
    }
    if let Some((math::RelationAggregateKind::FirstValue, relation, Some(selected), Some(fallback))) = value.as_relation_aggregate() {
        if let Some((range, alias)) = integer_domain(relation) {
            if selected.as_row_value() == Some(alias) {
                if let Some((_, end)) = range_bounds(range) {
                    return upper_bounded_by(end, limit, active) && upper_bounded_by(fallback, limit, active);
                }
            }
        }
    }
    false
}

fn indexed_domain<'a, 'program>(
    engine: &'a Engine<'program>, scope: &'a RangeScope, env: &Env,
    lower: &'a Expr, upper: &'a Expr, path: &'a Expr,
) -> Result<(RelationExpr, Env, Position<'a, 'program>, String)> {
    let alias = &scope.alias;
    let mut index = "i".to_owned();
    while engine.selected_entry.syntax.params().iter().any(|(_, public)| *public == index) { index.push('_'); }
    let position = Position { engine, alias, lower, upper, path };
    let mut local = env.clone();
    local.insert(alias.clone(), Value::Term(Ty::Integer, b::row_value(&index)));
    let mut domain = if math::normalize(b::ge(lower.clone(), upper.clone())).as_bool() == Some(true) {
        RelationExpr::Empty
    } else {
        RelationExpr::vector(math::domains::relation::positional::range(lower.clone(), upper.clone()), &index)
    };
    for filter in &scope.filters {
        let predicate = expression(filter, &local, Some(&position))?;
        need(predicate.ty() == Ty::Bool, "integer range filter requires a boolean")?;
        domain = RelationExpr::select(predicate.scalar()?, domain);
    }
    Ok((domain, local, position, index))
}

impl Engine<'_> {
    pub(super) fn source_integer_range(&self, range: &IntegerRange, env: &mut Env, path: &Expr, declared: bool) -> Result<()> {
        let target_error = "range operation requires a declared integer output";
        let prior = env.get(&range.binding);
        let initial = if range.accumulate {
            let prior = prior.ok_or_else(|| fail(target_error))?;
            need(prior.ty() == Ty::Integer, target_error)?;
            Some(prior.scalar()?)
        } else {
            need(prior.is_some_and(|value| value.ty() == Ty::Integer) || declared, target_error)?;
            None
        };
        let lower = integer(expression(&range.lower, env, None)?)?;
        let upper = integer(expression(&range.upper, env, None)?)?;
        let value = match &range.select {
            RangeOperation::Count => count(lower, upper),
            RangeOperation::FilteredCount(scope) => {
                let (domain, _, _, _) = indexed_domain(self, scope, env, &lower, &upper, path)?;
                r::relation_sum(domain, b::lit_int(1))
            }
            RangeOperation::Sum => interval_sum(lower, upper),
            RangeOperation::ProjectedSum { scope, value: term } => {
                let (domain, local, position, index) = indexed_domain(self, scope, env, &lower, &upper, path)?;
                let value = integer(expression(term, &local, Some(&position))?)?;
                if !scope.filters.is_empty() {
                    r::relation_sum(domain, value)
                } else if let Some(coefficients) = integer_polynomial(&value, &index) {
                    sum_polynomial(coefficients, lower, upper)
                } else {
                    r::relation_sum(domain, value)
                }
            }
            RangeOperation::First { scope, value, fallback } => {
                let (domain, local, position, _) = indexed_domain(self, scope, env, &lower, &upper, path)?;
                r::relation_first_value(domain, integer(expression(value, &local, Some(&position))?)?,
                    integer(expression(fallback, env, None)?)?)
            }
        };
        let value = match initial { Some(initial) => b::add(initial, value), None => value };
        env.insert(range.binding.clone(), Value::Term(Ty::Integer, math::normalize(value)));
        Ok(())
    }
}
