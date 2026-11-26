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

    fn check_source_bounds(&mut self, root: &str, index: i64, path: &Expr) -> Expr {
        let fails = math::normalize(bounds_failure(root, index));
        self.throw_source_on_path(b::and(path.clone(), fails.clone()), OUT_OF_BOUNDS);
        math::normalize(b::and(path.clone(), b::not(fails)))
    }

    pub(super) fn source_array_store(&mut self, array: Value, index: Value, rhs: Value, path: Expr) -> Result<()> {
        need(rhs.ty() == Ty::Integer, "source array update requires an exact integer value")?;
        let root = array_root(array)?;
        // Do not assume that separately supplied arrays are different
        // instances. Until cross-root alias analysis exists, a mutating source
        // entry may expose only one such root.
        let entry = self.selected_entry.ok_or_else(|| fail("array mutation requires a source entry"))?;
        let (owner, _) = entry.id.rsplit_once('.').ok_or_else(|| fail("missing source owner"))?;
        let parameters = entry.syntax.params().iter().filter(|(ty, _)| crate::normalized::numeric_array(ty)).count();
        let states = if entry.syntax.is_static() { 0 } else {
            self.program.find_class(owner).map_or(0, |class| class.fields().iter().filter(|field|
                crate::normalized::entry_state(class, field) && crate::normalized::numeric_array(field.java_type())).count())
        };
        need(parameters + states == 1, "array mutation needs one exposed root; distinct source arrays may alias")?;
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
            need(index_value(index)? == 0, "eager record projection currently supports only its first element")?;
            self.throw_source_on_path(b::and(path, b::not(r::relation_exists(base.clone()))), OUT_OF_BOUNDS);
            return Ok(Value::Term(Ty::Integer, r::relation_first_value(base, selected, b::lit_null())));
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
        let identity = self.source_array_reference(&root);
        let slot = self.realtime_store.read_with_initial(&identity, &index.to_string(), &guard, Some(original_slot(&root, index)))
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
        let identity = self.source_array_reference(&root);
        if let Some(indices) = self.source_array_writes.get(&root) {
            for &index in indices {
                let original = original_slot(&root, index);
                let bound = math::normalize(b::not(bounds_failure(&root, index)));
                let valid = math::normalize(b::and(path.clone(), bound.clone()));
                let updated = self.realtime_store.read_with_initial(&identity, &index.to_string(), &valid, Some(original.clone()))
                    .ok_or_else(|| fail("indexed observation has no value"))?;
                if updated == original { continue; }
                // Offset applies to every row before and after replacement, so
                // it cancels between the old and new contributions.
                if b::path_implies(&path, &bound) {
                    total = b::add(b::sub(total, original), updated);
                } else if indices.len() == 1 {
                    // One conditional overwrite selects between two complete
                    // aggregate observations of the same root. Reuse the
                    // shared CASE join used when a source return is already
                    // split. No slot read moves outside its bounds domain.
                    let adjusted = b::add(b::sub(total.clone(), original), updated);
                    total = realtime_state::join_value(bound, adjusted, total);
                } else {
                    // Keep multiple guarded corrections linear in the number
                    // of written indices rather than duplicating the sum tree.
                    let correction = b::case_when(vec![(bound, b::sub(updated, original))], b::lit_int(0));
                    total = b::add(total, correction);
                }
            }
        }
        Ok(Value::Term(Ty::Integer, math::normalize(total)))
    }
}
