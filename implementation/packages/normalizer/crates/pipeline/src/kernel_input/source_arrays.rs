//! Indexed operations on a directly bound exact-integer array. A Java alias
//! retains the original base relation; indexed writes share its store.
use super::*;

const OUT_OF_BOUNDS: &str = "java.lang.ArrayIndexOutOfBoundsException";
const NEGATIVE_SIZE: &str = "java.lang.NegativeArraySizeException";

#[derive(Clone, PartialEq)]
pub(super) enum LocalIntArray {
    Initialized(Vec<i32>),
    Zeroed(i32),
}

impl LocalIntArray {
    pub(super) fn length(&self) -> i64 {
        match self {
            Self::Initialized(values) => values.len() as i64,
            Self::Zeroed(length) => i64::from(*length),
        }
    }

    fn get(&self, index: i32) -> Option<i32> {
        if index < 0 || i64::from(index) >= self.length() {
            return None;
        }
        Some(match self {
            Self::Initialized(values) => values[index as usize],
            Self::Zeroed(_) => 0,
        })
    }
}

fn literal_integer(value: &Expr) -> Option<i64> {
    match value.as_lit() {
        Some(math::Lit::Int(value)) => Some(*value),
        _ => None,
    }
}

fn exact_java_int(value: Value) -> Result<i32> {
    need(value.ty() == Ty::Integer, "expected Java int for local array")?;
    let folded = math::normalize(value.scalar()?);
    let value = literal_integer(&folded)
        .ok_or_else(|| fail("local array operand must be a closed Java int constant"))?;
    i32::try_from(value).map_err(|_| fail("local array operand exceeds Java int range"))
}

fn array_root(value: Value) -> Result<String> {
    let Value::Sequence { item: Ty::Integer, name, .. } = value else {
        return Err(fail("operation requires a directly bound exact integer array"));
    };
    Ok(name)
}

fn index_value(index: Value) -> Result<i64> {
    need(index.ty() == Ty::Integer, "array subscript must be an exact integer")?;
    let value = literal_integer(&index.scalar()?)
        .ok_or_else(|| fail("array subscript must be a constant"))?;
    need((0..i64::from(i32::MAX)).contains(&value), "array subscript outside supported nonnegative Java index range")?;
    Ok(value)
}

fn initial_array(root: &str) -> RelationExpr {
    RelationExpr::base(root, "value")
}

pub(super) fn borrowed_array_length(root: &str) -> Expr {
    r::relation_count(initial_array(root))
}

fn bounds_failure(root: &str, index: i64) -> Expr {
    let length = borrowed_array_length(root);
    if index == 0 { b::eq(length, b::lit_int(0)) }
    else { b::le(length, b::lit_int(index)) }
}

fn original_slot(root: &str, index: i64) -> Expr {
    math::normalize(r::relation_nth_value(initial_array(root), b::lit_int(index)))
}

fn slot_delta(value: Expr, original: &Expr, path: &Expr) -> Expr {
    if value == *original {
        return b::lit_int(0);
    }
    // A guarded write only changes contribution where it occurred. Keep the
    // old slot read inside that branch, not in the no-write or OOB paths.
    if let Some(([(guard, written)], fallback)) = value.as_total_case_when() {
        if fallback == original {
            let mut pending = Vec::new();
            guard.for_each_conjunct(|term| {
                if !b::path_implies(path, term) { pending.push(term.clone()); }
            });
            let guard = pending.into_iter().fold(b::lit_bool(true), b::and);
            return b::case_when(vec![(guard, b::sub(written.clone(), original.clone()))], b::lit_int(0));
        }
    }
    b::sub(value, original.clone())
}

impl Engine<'_> {
    pub(super) fn source_local_array_initializer(&self, values: Vec<Value>) -> Result<LocalIntArray> {
        let entries = values.into_iter().map(exact_java_int).collect::<Result<_>>()?;
        Ok(LocalIntArray::Initialized(entries))
    }

    pub(super) fn source_local_array_allocation(&mut self, size: Value, path: Expr) -> Result<LocalIntArray> {
        let length = exact_java_int(size)?;
        if length < 0 {
            self.throw_source_on_path(path, NEGATIVE_SIZE);
        }
        Ok(LocalIntArray::Zeroed(length.max(0)))
    }

    fn source_array_reference(&mut self, root: &str) -> Expr {
        self.acquire_source_state_root(format!("array:{root}"))
    }

    fn array_roots_may_alias(&self, left: &str, right: &str) -> bool {
        if left == right { return true; }
        let Some(selected) = self.selected_entry else { return false; };
        let ty = |name: &str| selected.syntax.params().iter()
            .find(|(ty, param)| param == name && crate::normalized::numeric_array(ty))
            .map(|(ty, _)| ty);
        ty(left).is_some_and(|left| ty(right) == Some(left))
    }

    pub(super) fn source_array_may_have_writes(&self, root: &str) -> bool {
        self.source_array_writes.keys().any(|other| self.array_roots_may_alias(root, other))
    }

    // Input arrays with the same Java element type may denote the same
    // object. Use the public identity map rather than comparing heap numbers.
    // Other root kinds are not admitted to cross-root mutation below.
    fn array_alias_guard(&mut self, left: &str, right: &str) -> Option<Expr> {
        if left == right { return Some(b::lit_bool(true)); }
        if !self.array_roots_may_alias(left, right) { return None; }
        let (left, right) = if left <= right { (left, right) } else { (right, left) };
        let first = self.source_array_reference(left);
        let second = self.source_array_reference(right);
        for (reference, name) in [(&first, left), (&second, right)] {
            self.source_objects.compared_parameters.insert(reference.as_heap_ref()?, name.to_owned());
        }
        Some(b::eq(first, second))
    }

    fn array_slot_sources(&mut self, root: &str, index: i64) -> Vec<(Expr, Expr)> {
        let roots: Vec<_> = self.source_array_writes.iter()
            .filter_map(|(name, indices)| indices.contains(&index).then(|| name.clone()))
            .collect();
        roots.into_iter().filter_map(|other| {
            let aliases = self.array_alias_guard(root, &other)?;
            Some((self.source_array_reference(&other), aliases))
        }).collect()
    }

    fn check_source_bounds(&mut self, root: &str, index: i64, path: &Expr) -> Expr {
        let fails = math::normalize(bounds_failure(root, index));
        self.throw_source_on_path(b::and(path.clone(), fails.clone()), OUT_OF_BOUNDS);
        math::normalize(b::and(path.clone(), b::not(fails)))
    }

    pub(super) fn source_array_store(&mut self, array: Value, index: Value, rhs: Value, path: Expr) -> Result<()> {
        need(rhs.ty() == Ty::Integer, "source array update requires an exact integer value")?;
        let root = array_root(array)?;
        let entry = self.selected_entry.ok_or_else(|| fail("array mutation requires a source entry"))?;
        let (owner, _) = entry.id.rsplit_once('.').ok_or_else(|| fail("missing source owner"))?;
        let parameters = entry.syntax.params().iter().filter(|(ty, _)| crate::normalized::numeric_array(ty)).count();
        let states = if entry.syntax.is_static() { 0 } else {
            self.program.find_class(owner).map_or(0, |class| class.fields().iter().filter(|field|
                crate::normalized::entry_state(class, field) && crate::normalized::numeric_array(field.java_type())).count())
        };
        need(parameters + states == 1 || (states == 0 && entry.syntax.params().iter()
            .any(|(ty, name)| name == &root && crate::normalized::numeric_array(ty))),
            "array mutation needs statically bound parameter roots or a sole state root")?;
        let index = index_value(index)?;
        let value = rhs.scalar()?;
        let path = self.normal_path(path);
        let guard = self.check_source_bounds(&root, index, &path);
        if guard.as_bool() != Some(false) {
            let identity = self.source_array_reference(&root);
            self.realtime_store.store_guarded(guard.clone(), identity, index.to_string(), value.clone());
            self.source_array_writes.entry(root.clone()).or_default().insert(index);
            self.realtime_effects.write_array(guard, root, index, value);
        }
        Ok(())
    }

    pub(super) fn source_array_read(&mut self, array: Value, index: Value, path: Expr) -> Result<Value> {
        need(self.project.direct_source, "source array index requires direct source")?;
        if let Value::EagerIntProjection { base, selected } = array {
            let index = index_value(index)?;
            let RelationExpr::Base { name, .. } = &base else { return Err(fail("unbound record projection")); };
            // Projection has already evaluated, including null failures on any
            // input row. Only its normal path may reach this bounds check.
            let missing = if index == 0 { b::not(r::relation_exists(base.clone())) }
                else { bounds_failure(name, index) };
            self.throw_source_on_path(b::and(path, missing), OUT_OF_BOUNDS);
            let result = if index == 0 {
                r::relation_first_value(base, selected, b::lit_null())
            } else {
                r::relation_nth_value(RelationExpr::project(selected, "value", base), b::lit_int(index))
            };
            return Ok(Value::Term(Ty::Integer, result));
        }
        if let Value::LocalIntArray(local) = array {
            let index = exact_java_int(index)?;
            let Some(value) = local.get(index) else {
                self.throw_source_on_path(path, OUT_OF_BOUNDS);
                // This continuation is now unreachable; do not use null as a
                // normal result for an integer-valued Java expression.
                return Ok(Value::Term(Ty::Integer, b::lit_int(0)));
            };
            return Ok(Value::Term(Ty::Integer, b::lit_int(i64::from(value))));
        }
        let root = array_root(array)?;
        let index = index_value(index)?;
        let path = self.normal_path(path);
        let guard = self.check_source_bounds(&root, index, &path);
        let sources = self.array_slot_sources(&root, index);
        let slot = self.realtime_store.read_matching_with_initial(&index.to_string(), &guard, Some(original_slot(&root, index)),
            |reference| sources.iter().find(|(identity, _)| identity == reference).map(|(_, aliases)| aliases.clone()))
            .ok_or_else(|| fail("indexed observation has no value"))?;
        Ok(Value::Term(Ty::Integer, slot))
    }

    pub(super) fn source_array_sum(&mut self, array: Value, start: Expr, offset: i64, path: Expr) -> Result<Value> {
        need(self.project.direct_source, "source array sum requires direct source")?;
        let root = array_root(array)?;
        let path = self.normal_path(path);
        let each = b::row_value("value");
        let each = if offset == 0 { each } else { b::add(each, b::lit_int(offset)) };
        let mut total = b::add(start, r::relation_sum(initial_array(&root), each));
        let indices: BTreeSet<_> = self.source_array_writes.values().flatten().copied().collect();
        if !indices.is_empty() {
            for &index in &indices {
                let original = original_slot(&root, index);
                let bound = math::normalize(b::not(bounds_failure(&root, index)));
                let valid = math::normalize(b::and(path.clone(), bound.clone()));
                let sources = self.array_slot_sources(&root, index);
                let updated = self.realtime_store.read_matching_with_initial(&index.to_string(), &valid, Some(original.clone()),
                    |reference| sources.iter().find(|(identity, _)| identity == reference).map(|(_, aliases)| aliases.clone()))
                    .ok_or_else(|| fail("indexed observation has no value"))?;
                if updated == original { continue; }
                // Offset applies to every row before and after replacement, so
                // it cancels between the old and new contributions.
                let is_guarded_write = updated.as_total_case_when()
                    .is_some_and(|(_, fallback)| fallback == &original);
                if is_guarded_write {
                    // A successful same-object indexed write itself proves
                    // that the slot exists; its guard protects the old read.
                    total = b::add(total, slot_delta(updated, &original, &path));
                } else if b::path_implies(&path, &bound) {
                    total = b::add(b::sub(total, original), updated);
                } else {
                    // The input aggregate is acquired on both paths. Select
                    // only the indexed delta inside the bounds domain.
                    let correction = b::case_when(vec![(bound, b::sub(updated, original))], b::lit_int(0));
                    total = b::add(total, correction);
                }
            }
        }
        Ok(Value::Term(Ty::Integer, math::normalize(total)))
    }
}
