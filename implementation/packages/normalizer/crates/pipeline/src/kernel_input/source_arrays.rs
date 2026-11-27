//! Indexed operations on a directly bound exact-integer array. A Java alias
//! retains the original base relation; indexed writes share its store.
use super::*;

const OUT_OF_BOUNDS: &str = "java.lang.ArrayIndexOutOfBoundsException";
const NEGATIVE_SIZE: &str = "java.lang.NegativeArraySizeException";

#[derive(Clone, PartialEq)]
pub(super) enum LocalIntArray {
    Initialized { values: std::rc::Rc<[Expr]>, identity: i64 },
    Zeroed { length: i32, identity: i64 },
    Selection { guard: Expr, yes: std::rc::Rc<Self>, no: std::rc::Rc<Self> },
    Unreachable,
}

impl LocalIntArray {
    pub(super) fn select(guard: Expr, yes: Self, no: Self) -> Self {
        if yes == no { return yes; }
        match guard.as_bool() {
            Some(true) => yes,
            Some(false) => no,
            None => Self::Selection { guard, yes: yes.into(), no: no.into() },
        }
    }

    pub(super) fn length_expr(&self) -> Expr {
        match self {
            Self::Selection { guard, yes, no } => b::case_when(
                vec![(guard.clone(), yes.length_expr())], no.length_expr()),
            array => b::lit_int(array.length()),
        }
    }

    pub(super) fn length(&self) -> i64 {
        match self {
            Self::Initialized { values, .. } => values.len() as i64,
            Self::Zeroed { length, .. } => i64::from(*length),
            Self::Selection { .. } => unreachable!("select a concrete local identity before reading the length"),
            Self::Unreachable => 0,
        }
    }

    fn reference(&self) -> Option<Expr> {
        match self {
            Self::Initialized { identity, .. } | Self::Zeroed { identity, .. } =>
                Some(b::heap_ref(*identity)),
            Self::Selection { .. } | Self::Unreachable => None,
        }
    }

    fn get(&self, index: i64) -> Option<Expr> {
        if !(0..self.length()).contains(&index) { return None; }
        match self {
            Self::Initialized { values, .. } => Some(values[index as usize].clone()),
            Self::Zeroed { .. } => Some(b::lit_int(0)),
            Self::Selection { .. } | Self::Unreachable => None,
        }
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
    need(i32::try_from(value).is_ok(), "array subscript outside Java int range")?;
    Ok(value)
}

fn initial_array(root: &str) -> RelationExpr {
    RelationExpr::base(root, "value")
}

pub(super) fn borrowed_array_length(root: &str) -> Expr {
    r::relation_count(initial_array(root))
}

fn bounds_failure(root: &str, index: i64) -> Expr {
    if index < 0 { return b::lit_bool(true); }
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
    pub(super) fn source_local_array_initializer(&mut self, values: Vec<Value>) -> Result<LocalIntArray> {
        let entries = values.into_iter().map(|value| {
            need(value.ty() == Ty::Integer, "expected Java int for local array")?;
            value.scalar()
        }).collect::<Result<Vec<_>>>()?;
        Ok(LocalIntArray::Initialized { values: entries.into(), identity: self.heap.new_object().id() })
    }

    pub(super) fn source_local_array_allocation(&mut self, size: Value, path: Expr) -> Result<LocalIntArray> {
        let length = exact_java_int(size)?;
        if length < 0 {
            self.throw_source_on_path(path, NEGATIVE_SIZE);
            return Ok(LocalIntArray::Unreachable);
        }
        Ok(LocalIntArray::Zeroed { length, identity: self.heap.new_object().id() })
    }

    pub(super) fn snapshot_local_choice(&mut self, array: &LocalIntArray, path: &Expr) -> Result<Vec<Expr>> {
        self.budget()?;
        if let LocalIntArray::Selection { guard, yes, no } = array {
            return match self.condition_under_path(guard.clone(), path).as_bool() {
                Some(true) => self.snapshot_local_choice(yes, path),
                Some(false) => self.snapshot_local_choice(no, path),
                None => {
                    let a = self.snapshot_local_choice(yes, &b::and(path.clone(), guard.clone()))?;
                    let b = self.snapshot_local_choice(no, &b::and(path.clone(), b::not(guard.clone())))?;
                    need(a == b, "conditional local array choice needs a closed candidate snapshot for each path")?;
                    Ok(a)
                }
            };
        }
        let reference = array.reference().ok_or_else(|| fail("local array identity is not acquired"))?;
        let positions = self.source_local_array_positions(array, &reference);
        let remaining_zeroes = matches!(array, LocalIntArray::Zeroed { .. })
            && (positions.len() as i64) < array.length();
        let mut values = Vec::new();
        if remaining_zeroes { values.push(b::lit_int(0)); }
        for slot in positions {
            self.budget()?;
            values.push(self.read_local_array_slot(array, &reference, slot, path)?);
        }
        Ok(values)
    }

    fn source_array_reference(&mut self, root: &str) -> Expr {
        self.acquire_source_state_root(format!("array:{root}"))
    }

    fn array_roots_may_alias(&self, left: &str, right: &str) -> bool {
        if left == right { return true; }
        let ty = |name: &str| self.selected_entry.syntax.params().iter()
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
        let path = self.normal_path(path);
        if path.as_bool() == Some(false) { return Ok(()); }
        need(rhs.ty() == Ty::Integer, "source array update requires an exact integer value")?;
        if let Value::LocalIntArray(local) = array {
            if let LocalIntArray::Selection { guard, yes, no } = local {
                self.budget()?;
                self.source_array_store(Value::LocalIntArray((*yes).clone()), index.clone(), rhs.clone(),
                    b::and(path.clone(), guard.clone()))?;
                return self.source_array_store(Value::LocalIntArray((*no).clone()), index, rhs,
                    b::and(path, b::not(guard)));
            }
            let index = index_value(index)?;
            let reference = local.reference().ok_or_else(|| fail("local array identity is not acquired"))?;
            if local.get(index).is_none() {
                self.throw_source_on_path(path, OUT_OF_BOUNDS);
            } else {
                // Aliases hold the same heap reference. This nonescaping
                // store is visible to the invocation, not published as S.
                self.realtime_store.store_guarded(path, reference, index.to_string(), rhs.scalar()?);
            }
            return Ok(());
        }
        let root = array_root(array)?;
        let entry = self.selected_entry;
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
        if let Value::Rows(rows) = array {
            return self.index_ordered_table_array(*rows, exact_java_int(index)?, path);
        }
        if let Value::EagerIntProjection { source, field } = array {
            let index = index_value(index)?;
            if index < 0 {
                self.throw_source_on_path(path, OUT_OF_BOUNDS);
                return Ok(Value::Term(Ty::Integer, b::lit_int(0)));
            }
            let base = RelationExpr::base(&source, "r");
            // Projection has already evaluated, including null failures on any
            // input row. Only its normal path may reach this bounds check.
            let missing = if index == 0 { b::not(r::relation_exists(base.clone())) }
                else { bounds_failure(&source, index) };
            self.throw_source_on_path(b::and(path, missing), OUT_OF_BOUNDS);
            let result = if index == 0 {
                r::relation_first_value(base, b::row_value(format!("r.{field}")), b::lit_null())
            } else {
                r::relation_nth_value(RelationExpr::RecordProjection { source, field }, b::lit_int(index))
            };
            return Ok(Value::Term(Ty::Integer, result));
        }
        if let Value::LocalIntArray(local) = array {
            need(index.ty() == Ty::Integer, "array subscript must be an exact integer")?;
            return self.source_local_array_read(&local, index.scalar()?, path);
        }
        let root = array_root(array)?;
        need(index.ty() == Ty::Integer, "array subscript must be an exact integer")?;
        let expression = math::normalize(index.scalar()?);
        let path = self.normal_path(path);
        let Some(index) = literal_integer(&expression) else {
            need(expression.as_lit().is_none(), "array subscript outside Java int range")?;
            return self.read_symbolic_array_slot(&root, expression, path);
        };
        need(i32::try_from(index).is_ok(), "array subscript outside Java int range")?;
        let guard = self.check_source_bounds(&root, index, &path);
        if guard.as_bool() == Some(false) {
            return Ok(Value::Term(Ty::Integer, b::lit_int(0)));
        }
        let sources = self.array_slot_sources(&root, index);
        let slot = self.realtime_store.read_matching_with_initial(&index.to_string(), &guard, Some(original_slot(&root, index)),
            |reference| sources.iter().find(|(identity, _)| identity == reference).map(|(_, aliases)| aliases.clone()))
            .ok_or_else(|| fail("indexed observation has no value"))?;
        Ok(Value::Term(Ty::Integer, slot))
    }

    fn source_local_array_read(&mut self, array: &LocalIntArray, index: Expr, path: Expr) -> Result<Value> {
        if let LocalIntArray::Selection { guard, yes, no } = array {
            return self.observe_local_array_selection(guard, yes, no, path, |engine, arm, domain|
                engine.source_local_array_read(arm, index.clone(), domain));
        }
        let length = array.length();
        let outside = if length == 0 { b::lit_bool(true) } else {
            b::or(b::lt(index.clone(), b::lit_int(0)), b::ge(index.clone(), b::lit_int(length)))
        };
        self.throw_source_on_path(b::and(path.clone(), outside), OUT_OF_BOUNDS);
        let live = self.normal_path(path);
        if live.as_bool() == Some(false) { return Ok(Value::Term(Ty::Integer, b::lit_null())); }
        let reference = array.reference().ok_or_else(|| fail("local array identity is not acquired"))?;
        let value = if let Some(slot) = literal_integer(&index) {
            self.read_local_array_slot(array, &reference, slot, &live)?
        } else {
            let positions = self.source_local_array_positions(array, &reference);
            let mut arms = Vec::new();
            for position in positions {
                self.budget()?;
                let selected = b::eq(index.clone(), b::lit_int(position));
                let domain = math::normalize(b::and(live.clone(), selected.clone()));
                if domain.as_bool() == Some(false) { continue; }
                arms.push((selected, self.read_local_array_slot(array, &reference, position, &domain)?));
            }
            let fallback = match array {
                LocalIntArray::Initialized { .. } => arms.pop()
                    .ok_or_else(|| fail("local array read has no live element on the bounds domain"))?.1,
                LocalIntArray::Zeroed { .. } => b::lit_int(0),
                LocalIntArray::Selection { .. } | LocalIntArray::Unreachable =>
                    unreachable!("read needs the selected concrete identity"),
            };
            self.normalize_execution_value(b::case_when(arms, fallback))
        };
        Ok(Value::Term(Ty::Integer, value))
    }

    pub(super) fn local_array_identity_eq(&mut self, left: &LocalIntArray, right: &LocalIntArray) -> Result<Expr> {
        self.budget()?;
        if left == right { return Ok(b::lit_bool(true)); }
        let result = match (left, right) {
            (LocalIntArray::Selection { guard, yes, no }, _) => b::case_when(
                vec![(guard.clone(), self.local_array_identity_eq(yes, right)?)],
                self.local_array_identity_eq(no, right)?,
            ),
            (_, LocalIntArray::Selection { guard, yes, no }) => b::case_when(
                vec![(guard.clone(), self.local_array_identity_eq(left, yes)?)],
                self.local_array_identity_eq(left, no)?,
            ),
            _ => {
                let reference = |array: &LocalIntArray| array.reference()
                    .ok_or_else(|| fail("array comparison requires an acquired local identity"));
                // These references come from actual simulated allocations.
                // Their contents and the order of allocation do not decide
                // equality, and no internal address is published.
                b::lit_bool(reference(left)? == reference(right)?)
            }
        };
        Ok(self.normalize_execution_value(result))
    }

    fn read_local_array_slot(&self, array: &LocalIntArray, reference: &Expr, slot: i64, domain: &Expr) -> Result<Expr> {
        let initial = array.get(slot).ok_or_else(|| fail("local array slot outside the selected domain"))?;
        self.realtime_store.read_with_initial(reference, &slot.to_string(), domain, Some(initial))
            .ok_or_else(|| fail("uninitialized local array element"))
    }

    fn source_local_array_positions(&self, array: &LocalIntArray, reference: &Expr) -> BTreeSet<i64> {
        match array {
            LocalIntArray::Initialized { .. } => (0..array.length()).collect(),
            // Only written positions can depart from zero, even when the
            // allocated array would be too large to enumerate.
            LocalIntArray::Zeroed { .. } => self.realtime_store.entries().iter()
                .filter(|entry| entry.reference == *reference)
                .filter_map(|entry| entry.field.parse::<i64>().ok())
                .collect(),
            LocalIntArray::Selection { .. } | LocalIntArray::Unreachable =>
                unreachable!("a concrete local identity is required"),
        }
    }

    // The selection has already been evaluated. Dispatch only the current
    // observation, preserving the original array identity in each arm.
    fn observe_local_array_selection(
        &mut self, guard: &Expr, yes: &LocalIntArray, no: &LocalIntArray, path: Expr,
        mut observe: impl FnMut(&mut Self, &LocalIntArray, Expr) -> Result<Value>,
    ) -> Result<Value> {
        self.budget()?;
        let mut on = |array, path| -> Result<Option<Value>> {
            let path = self.normal_path(path);
            if path.as_bool() == Some(false) { return Ok(None); }
            let value = observe(self, array, path.clone())?;
            Ok((self.normal_path(path).as_bool() != Some(false)).then_some(value))
        };
        let yes_value = on(yes, b::and(path.clone(), guard.clone()))?;
        let no_value = on(no, b::and(path, b::not(guard.clone())))?;
        match (yes_value, no_value) {
            (Some(yes), Some(no)) => self.choose(guard.clone(), yes, no),
            (Some(value), None) | (None, Some(value)) => Ok(value),
            (None, None) => Ok(Value::Term(Ty::Integer, b::lit_null())),
        }
    }

    fn read_symbolic_array_slot(&mut self, root: &str, index: Expr, path: Expr) -> Result<Value> {
        let outside = b::or(b::lt(index.clone(), b::lit_int(0)),
            b::ge(index.clone(), borrowed_array_length(root)));
        self.throw_source_on_path(b::and(path.clone(), outside), OUT_OF_BOUNDS);
        let live = self.normal_path(path);
        if live.as_bool() == Some(false) {
            return Ok(Value::Term(Ty::Integer, b::lit_int(0)));
        }
        let original = r::relation_nth_value(initial_array(root), index.clone());
        let positions: BTreeSet<_> = self.source_array_writes.values().flatten().copied().collect();
        let mut arms = Vec::new();
        for position in positions {
            let selected = b::eq(index.clone(), b::lit_int(position));
            let active = math::normalize(b::and(live.clone(), selected.clone()));
            if active.as_bool() == Some(false) { continue; }
            let sources = self.array_slot_sources(root, position);
            if sources.is_empty() { continue; }
            let initial = original_slot(root, position);
            let updated = self.realtime_store.read_matching_with_initial(&position.to_string(), &active, Some(initial.clone()),
                |reference| sources.iter().find(|(identity, _)| identity == reference).map(|(_, alias)| alias.clone()))
                .ok_or_else(|| fail("indexed observation has no value"))?;
            if updated != initial { arms.push((selected, updated)); }
        }
        // The index selections are disjoint. The fallback is the original
        // slot of the same snapshot on the already checked normal domain.
        let value = if arms.is_empty() { original } else { b::case_when(arms, original) };
        Ok(Value::Term(Ty::Integer, math::normalize(value)))
    }

    pub(super) fn source_array_sum(&mut self, array: Value, start: Expr, offset: i64, path: Expr) -> Result<Value> {
        need(self.project.direct_source, "source array sum requires direct source")?;
        let path = self.normal_path(path);
        if let Value::LocalIntArray(local) = array {
            if let LocalIntArray::Selection { guard, yes, no } = local {
                return self.observe_local_array_selection(&guard, &yes, &no, path, |engine, arm, domain|
                    engine.source_array_sum(Value::LocalIntArray(arm.clone()), start.clone(), offset, domain));
            }
            let reference = local.reference().ok_or_else(|| fail("local array identity is not acquired"))?;
            // Contribution from the fixed per-element offset is independent
            // of slot contents. Read the actual shared store for each other
            // contribution at the loop's entry on this path.
            let adjustment = math::Integer::from(offset) * local.length();
            let mut total = b::add(start, b::lit_integer(adjustment));
            for slot in self.source_local_array_positions(&local, &reference) {
                self.budget()?;
                total = b::add(total, self.read_local_array_slot(&local, &reference, slot, &path)?);
            }
            return Ok(Value::Term(Ty::Integer, self.normalize_execution_value(total)));
        }
        let root = array_root(array)?;
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
