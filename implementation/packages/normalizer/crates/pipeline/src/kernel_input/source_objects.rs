//! Typed business object identities share the execution store. A borrowed
//! object's scalar content keeps its input origin, while writes to a fresh
//! object are projected only when its content crosses the entry boundary.
use super::*;

#[derive(Clone)]
enum Origin {
    Parameter(String),
    State { binding: u64, path: Vec<String> },
    Allocation,
}

#[derive(Default)]
pub(super) struct SourceObjects {
    objects: BTreeMap<i64, (String, Origin)>,
    states: BTreeMap<(String, String), Expr>,
    borrowed_fields: BTreeMap<(i64, String), i64>,
    pub(super) state_bindings: BTreeMap<u64, String>,
    pub(super) compared_parameters: BTreeMap<i64, String>,
    pub(super) compared_states: BTreeMap<i64, u64>,
    nullable_parameters: BTreeSet<i64>,
}
impl SourceObjects {
    pub(super) fn allocation_owner(&self, reference: &Expr) -> Option<&str> {
        let (owner, origin) = self.objects.get(&reference.as_heap_ref()?)?;
        matches!(origin, Origin::Allocation).then_some(owner.as_str())
    }

    pub(super) fn borrowed_parameter(&self, reference: &Expr) -> Option<(&str, &str)> {
        let (owner, Origin::Parameter(name)) = self.objects.get(&reference.as_heap_ref()?)? else {
            return None;
        };
        Some((owner, name))
    }

    // Fresh local allocations cannot equal an identity acquired from the
    // entry snapshot. Private heap IDs alone do not prove this relation.
    fn fresh_and_acquired(&self, left: &Expr, right: &Expr) -> bool {
        let origin = |value: &Expr| {
            value.as_heap_ref().and_then(|id| self.objects.get(&id)).map(|(_, origin)| origin)
        };
        matches!((origin(left), origin(right)),
            (Some(Origin::Allocation), Some(Origin::Parameter(_) | Origin::State { .. }))
            | (Some(Origin::Parameter(_) | Origin::State { .. }), Some(Origin::Allocation)))
    }

    pub(super) fn owns(&self, reference: &Expr) -> bool {
        reference
            .as_heap_ref()
            .is_some_and(|id| self.objects.contains_key(&id))
    }
}

impl Engine<'_> {
    pub(super) fn entry_argument(
        &mut self,
        name: &str,
        declared: &JavaType,
        ty: &Ty,
        nullable: bool,
    ) -> Result<Value> {
        if let JavaType::Array(item) = declared {
            if let Some(owner) = self.project.source_owner(item) {
                self.source_relations.record_inputs.insert(name.into(), owner.into());
            }
        }
        if let Some(owner) = self.project.source_owner(declared) {
            let reference = self.heap.new_path_object();
            self.source_objects.objects.insert(
                reference.id(),
                (owner.into(), Origin::Parameter(name.into())),
            );
            if nullable { self.source_objects.nullable_parameters.insert(reference.id()); }
            Ok(Value::Term(Ty::Ref, b::heap_ref(reference.id())))
        } else {
            symbolic(name, ty, &mut self.heap)
        }
    }

    pub(super) fn is_entry_state(&self, owner: &str, name: &str) -> bool {
        self.program.find_class(owner).is_some_and(|class| {
            class.fields().iter().any(|f| f.name() == name && crate::normalized::entry_state(class, f))
        })
    }

    fn initial_entry_state(&mut self, owner: &str, name: &str) -> Result<Value> {
        let class = self
            .program
            .find_class(owner)
            .ok_or_else(|| fail("missing receiver declaration"))?;
        let field = class
            .fields()
            .iter()
            .find(|f| f.name() == name && crate::normalized::entry_state(class, f))
            .ok_or_else(|| fail(format!("{owner}.{name} requires a declared entry state")))?;
        let ty = self.project.parameter_type(field.java_type())?;
        if crate::normalized::numeric_array(field.java_type()) {
            let symbol = source_state_symbol(name);
            if !self.source_objects.state_bindings.values().any(|root| root == name) {
                let binding = self.fresh_binding();
                self.source_objects.state_bindings.insert(binding, name.into());
            }
            return symbolic(&symbol, &ty, &mut self.heap);
        }
        let key = (owner.to_string(), name.to_string());
        if let Some(value) = self.source_objects.states.get(&key) {
            return Ok(Value::Term(ty, value.clone()));
        }
        let bind = self.fresh_binding();
        self.source_objects.state_bindings.insert(bind, name.into());
        let value = if let Some(target) = self.project.source_owner(field.java_type()) {
            let reference = self
                .heap
                .new_path_object();
            self.source_objects
                .objects
                .insert(reference.id(), (target.into(), Origin::State { binding: bind, path: Vec::new() }));
            let reference = b::heap_ref(reference.id());
            self.state_roots.insert(format!("{owner}.{name}"), reference.clone());
            reference
        } else {
            let primitive = scalar_origin_type(&ty)?;
            b::param(source_state_symbol(name), primitive)
        };
        self.source_objects.states.insert(key, value.clone());
        Ok(Value::Term(ty, value))
    }

    fn scalar_state_reference(&mut self, owner: &str, name: &str) -> Expr {
        let key = format!("{owner}.{name}");
        if let Some(reference) = self.state_roots.get(&key) { return reference.clone(); }
        let reference = self.heap.new_path_object();
        let reference = b::heap_ref(reference.id());
        self.state_roots.insert(key, reference.clone());
        reference
    }

    pub(super) fn read_entry_state(&mut self, owner: &str, name: &str, path: &Expr) -> Result<Value> {
        let initial = self.initial_entry_state(owner, name)?;
        if matches!(initial.ty(), Ty::Ref | Ty::Seq(_)) { return Ok(initial); }
        let root = self.scalar_state_reference(owner, name);
        let value = self.realtime_store.read_with_initial(&root, name, path, Some(initial.scalar()?))
            .ok_or_else(|| fail("missing entry state initial value"))?;
        Ok(Value::Term(initial.ty(), value))
    }

    pub(super) fn write_entry_state(
        &mut self, owner: &str, name: &str, value: Value, path: Expr,
    ) -> Result<()> {
        let initial = self.initial_entry_state(owner, name)?;
        need(initial.ty() != Ty::Ref && value.ty() == initial.ty(), "entry scalar state assignment type mismatch")?;
        let value = value.scalar()?;
        let guard = self.normal_path(path);
        let root = self.scalar_state_reference(owner, name);
        self.realtime_store.store_guarded(guard.clone(), root, name, value.clone());
        self.realtime_effects.write_state(guard, &[name.into()], value);
        Ok(())
    }

    pub(super) fn construct_source_object(
        &mut self,
        owner: &str,
        args: Vec<Value>,
        path: Expr,
    ) -> Result<Value> {
        let class = self
            .program
            .find_class(owner)
            .ok_or_else(|| fail("undeclared constructor owner"))?;
        let reference = self.heap.new_object();
        let r = b::heap_ref(reference.id());
        self.realtime_local_allocations.insert(reference.id());
        self.source_objects
            .objects
            .insert(reference.id(), (owner.into(), Origin::Allocation));
        for field in class.fields() {
            let default = match self.project.parameter_type(field.java_type())? {
                Ty::Integer => b::lit_int(0),
                Ty::Bool => b::lit_bool(false),
                Ty::Ref | Ty::Text => b::lit_null(),
                _ => return Err(fail("unsupported source field initializer")),
            };
            self.realtime_store
                .store_guarded(path.clone(), r.clone(), field.name(), default);
        }
        let value = Value::Term(Ty::Ref, r);
        if let Some(constructor) = class.methods().iter().find(|m| m.is_constructor()) {
            self.java_call_on(
                &format!("{owner}.{}", constructor.name()),
                args,
                path,
                Some(value.clone()),
            )?;
        } else {
            need(args.is_empty(), "implicit constructor accepts no arguments")?;
        }
        Ok(value)
    }

    // A borrowed child keeps all of its original P/E path. Give it an
    // internal identity so a local alias still denotes the same path.
    pub(super) fn read_or_bind_source_field(
        &mut self, reference: &Expr, name: &str, path: &Expr,
    ) -> Result<Value> {
        let (id, owner, origin) = reference.as_heap_ref()
            .and_then(|id| self.source_objects.objects.get(&id).map(|(owner, origin)| (id, owner, origin)))
            .ok_or_else(|| fail("unknown source receiver"))?;
        let Some((class, field, child_owner)) = self.program.find_class(owner)
            .and_then(|class| class.fields().iter().find(|field| field.name() == name)
                .and_then(|field| self.project.source_owner(field.java_type()).map(|child| (class, field, child))))
        else { return self.read_guarded_source_scalar(reference, name, path); };
        need(!crate::normalized::source_nullable_field(class, field),
            "ordinary object field read has no nullable member acquisition proof")?;
        let child = match origin {
            Origin::Allocation => return self.read_source_field_on_path(reference, name, path),
            Origin::Parameter(root) => Origin::Parameter(format!("{root}.{name}")),
            Origin::State { binding, path } => {
                let mut path = path.clone();
                path.push(name.to_owned());
                Origin::State { binding: *binding, path }
            }
        };
        let key = (id, name.to_owned());
        if let Some(child) = self.source_objects.borrowed_fields.get(&key) {
            return Ok(Value::Term(Ty::Ref, b::heap_ref(*child)));
        }
        let child_ref = self.heap.new_path_object().id();
        self.source_objects.objects.insert(child_ref, (child_owner.to_owned(), child));
        self.source_objects.borrowed_fields.insert(key, child_ref);
        Ok(Value::Term(Ty::Ref, b::heap_ref(child_ref)))
    }

    pub(super) fn source_object_null(&mut self, reference: &Expr) -> Expr {
        if let Some(id) = reference.as_heap_ref().filter(|id| self.source_objects.nullable_parameters.contains(id)) {
            let (_, Origin::Parameter(name)) = &self.source_objects.objects[&id] else { unreachable!() };
            self.source_objects.compared_parameters.insert(id, name.clone());
            b::eq(reference.clone(), b::lit_null())
        } else {
            b::lit_bool(false)
        }
    }

    fn read_guarded_source_scalar(&mut self, reference: &Expr, field: &str, path: &Expr) -> Result<Value> {
        let id = reference.as_heap_ref().ok_or_else(|| fail("unresolved source receiver"))?;
        if matches!(self.source_objects.objects[&id].1, Origin::Allocation) {
            return self.read_source_field_on_path(reference, field, path);
        }
        let initial = self.read_source_field_on_path(reference, field, path)?;
        let path = self.normal_path(path.clone());
        let mut identities = BTreeMap::new();
        let candidates = self.realtime_store.entries().iter()
            .filter(|entry| entry.field == field && entry.reference != *reference)
            .filter_map(|entry| entry.reference.as_heap_ref())
            .filter(|other| self.source_objects.objects.get(other).is_some_and(|(owner, origin)| {
                owner == &self.source_objects.objects[&id].0 && matches!(origin, Origin::State { .. })
            }))
            .collect::<BTreeSet<_>>();
        for other in candidates {
            let candidate = b::heap_ref(other);
            self.check_borrowed_identity(reference, &candidate)?;
            identities.insert(other, b::eq(reference.clone(), candidate));
        }
        let value = self.realtime_store.read_matching_with_initial(field, &path, Some(initial.scalar()?), |candidate| {
            if candidate == reference { Some(b::lit_bool(true)) }
            else { candidate.as_heap_ref().and_then(|id| identities.get(&id).cloned()) }
        }).ok_or_else(|| fail("unresolved state read"))?;
        Ok(Value::Term(initial.ty(), value))
    }

    pub(super) fn read_source_field(&self, reference: &Expr, name: &str) -> Result<Value> {
        self.read_source_field_on_path(reference, name, &b::lit_bool(true))
    }

    pub(super) fn read_source_field_on_path(&self, reference: &Expr, name: &str, path: &Expr) -> Result<Value> {
        let (owner, origin) = self
            .source_objects
            .objects
            .get(
                &reference
                    .as_heap_ref()
                    .ok_or_else(|| fail("unresolved source receiver"))?,
            )
            .ok_or_else(|| fail("unknown source receiver"))?;
        let field = self
            .program
            .find_class(owner)
            .and_then(|c| c.fields().iter().find(|f| f.name() == name))
            .ok_or_else(|| fail("unknown source field"))?;
        let ty = self.project.parameter_type(field.java_type())?;
        let expr = match origin {
            Origin::Allocation => self
                .realtime_store
                .read_on_path(reference, name, path)
                .ok_or_else(|| fail("uninitialized source field"))?,
            Origin::Parameter(root) => b::param(format!("{root}.{name}"), scalar_origin_type(&ty)?),
            Origin::State { binding, path } => {
                let primitive = scalar_origin_type(&ty)?;
                path.iter().map(String::as_str).chain([name]).fold(b::sym_e(*binding, primitive), |base, field| {
                    b::sym_e_project(&base, field, primitive)
                })
            }
        };
        Ok(Value::Term(ty, expr))
    }

    pub(super) fn write_source_field(
        &mut self,
        reference: &Expr,
        field: &str,
        value: Value,
        path: Expr,
    ) -> Result<()> {
        let id = reference
            .as_heap_ref()
            .ok_or_else(|| fail("unresolved source receiver"))?;
        let (owner, origin) = &self.source_objects.objects[&id];
        let state = match origin {
            Origin::Allocation => None,
            Origin::State { binding, path } if path.is_empty() => {
                Some(self.source_objects.state_bindings[binding].clone())
            }
            _ => return Err(fail("observable mutation of an input object requires a state write")),
        };
        let declaration = self
            .program
            .find_class(owner)
            .and_then(|c| c.fields().iter().find(|f| f.name() == field))
            .ok_or_else(|| fail("unknown assigned field"))?;
        need(
            value.ty() == self.project.parameter_type(declaration.java_type())?,
            "field assignment type mismatch",
        )?;
        let path = self.normal_path(path);
        let value = value.scalar()?;
        if let Some(state) = state {
            scalar_origin_type(&self.project.parameter_type(declaration.java_type())?)?;
            self.realtime_effects.write_state(path.clone(), &[state, field.into()], value.clone());
        }
        self.realtime_store.store_guarded(path, reference.clone(), field, value);
        Ok(())
    }

    pub(super) fn source_reference_equality(&mut self, left: Expr, right: Expr, different: bool) -> Result<Expr> {
        // Both operands have been evaluated. A complete conditional local
        // identity can be compared branchwise with an already acquired root
        // without re-running a branch or comparing its object's contents.
        let selected = match (left.as_case_when(), right.as_case_when()) {
            (Some(arms), None) if self.source_objects.owns(&right) => Some((arms, &right, true)),
            (None, Some(arms)) if self.source_objects.owns(&left) => Some((arms, &left, false)),
            _ => None,
        };
        if let Some((arms, other, selected_left)) = selected {
            need(arms.last().is_some_and(|(guard, _)| guard.as_bool() == Some(true)),
                "conditional reference requires a complete alias partition")?;
            let mut result = Vec::with_capacity(arms.len());
            for (guard, value) in arms {
                need(self.source_objects.owns(value), "conditional source identity requires a bound reference")?;
                let (a, z) = if selected_left { (value, other) } else { (other, value) };
                result.push((guard.clone(), self.compare_reference_roots(a.clone(), z.clone(), different)?));
            }
            return Ok(math::normalize(b::case_when(result)));
        }
        self.compare_reference_roots(left, right, different)
    }

    fn compare_reference_roots(&mut self, mut left: Expr, mut right: Expr, different: bool) -> Result<Expr> {
        if self.source_objects.fresh_and_acquired(&left, &right) { return Ok(b::lit_bool(different)); }
        self.check_borrowed_identity(&left, &right)?;
        let kind = if different { math::BinOpKind::Ne } else { math::BinOpKind::Eq };
        let names = crate::canonical_names::CanonicalNames {
            heap: &self.source_objects.compared_parameters, external: None, total_temporals: None, temporals: None,
        };
        if math::canonical_order_ordered_comparison(kind, &left, &right, &names).is_some() {
            std::mem::swap(&mut left, &mut right);
        }
        Ok(if different { b::ne(left, right) } else { b::eq(left, right) })
    }

    pub(super) fn check_borrowed_identity(&mut self, a: &Expr, b: &Expr) -> Result<()> {
        if a == b {
            return Ok(());
        }
        if [a, b].into_iter().any(|expr| {
            expr.as_heap_ref()
                .is_some_and(|id| self.source_relations.first_rows.contains_key(&id))
        }) {
            return Err(fail(
                "selected row identity comparison requires an alias contract",
            ));
        }
        let origin = |e: &Expr| {
            e.as_heap_ref()
                .and_then(|id| self.source_objects.objects.get(&id))
                .map(|(_, origin)| origin)
        };
        match (origin(a), origin(b)) {
            (Some(Origin::Allocation), _) | (_, Some(Origin::Allocation)) | (None, None) => Ok(()),
            (Some(left), Some(right)) if [left, right].into_iter().all(|origin| match origin {
                Origin::State { path, .. } => path.is_empty(),
                Origin::Parameter(name) => !name.contains('.'),
                Origin::Allocation => false,
            }) => {
                // A different borrowed ID describes a different acquisition
                // path, not necessarily a different Java object.
                for (reference, origin) in [(a, left), (b, right)] {
                    let id = reference.as_heap_ref().unwrap();
                    match origin {
                        Origin::Parameter(name) => { self.source_objects.compared_parameters.insert(id, name.clone()); }
                        Origin::State { binding, .. } => { self.source_objects.compared_states.insert(id, *binding); }
                        Origin::Allocation => unreachable!(),
                    }
                }
                Ok(())
            }
            _ => Err(fail(
                "borrowed identity comparison requires an alias contract",
            )),
        }
    }

    fn check_source_content_cycle(
        &self, root: &Expr, active: &mut BTreeSet<i64>, completed: &mut BTreeSet<i64>,
    ) -> Result<()> {
        let Some(id) = root.as_heap_ref() else { return Ok(()); };
        let Some((owner, Origin::Allocation)) = self.source_objects.objects.get(&id)
            else { return Ok(()); };
        if completed.contains(&id) { return Ok(()); }
        if !active.insert(id) {
            return Err(format!("[NORM-CYCLIC-CONTENT-EXIT] kind=ABORT location={} problem=cyclic content at the return boundary",
                self.selected_entry.unwrap_or("source")));
        }
        let class = self.program.find_class(owner).ok_or("missing object return layout")?;
        for field in class.fields() {
            if self.project.parameter_type(field.java_type())? == Ty::Ref {
                let value = self.read_source_field(root, field.name())?.scalar()?;
                value.as_heap_ref().map(|_| self.check_source_content_cycle(&value, active, completed)).transpose()?;
            }
        }
        active.remove(&id);
        completed.insert(id);
        Ok(())
    }

    pub(super) fn source_record(
        &self,
        declared: &JavaType,
        value: &Value,
    ) -> Result<Option<Vec<(String, Expr)>>> {
        let Some(owner) = self.project.source_owner(declared) else {
            return Ok(None);
        };
        let reference = value.scalar()?;
        if !self.source_objects.owns(&reference) {
            return Ok(None);
        }
        need(
            self.realtime_completion.return_value().as_ref() == Some(&reference)
                && self.realtime_throws.is_empty(),
            "object projection requires a total normal return of the same object",
        )?;
        self.check_source_content_cycle(&reference, &mut BTreeSet::new(), &mut BTreeSet::new())?;
        let class = self
            .program
            .find_class(owner)
            .ok_or_else(|| fail("missing object return layout"))?;
        let fields = class
            .fields()
            .iter()
            .map(|f| {
                let field = self.read_source_field(&reference, f.name())?;
                let scalar = field.scalar()?;
                if field.ty() == Ty::Text && !crate::normalized::nullable_text_field(class, f) {
                    need(!scalar.is_null(), "nonnullable record text field was not initialized")?;
                }
                Ok((f.name().into(), scalar))
            })
            .collect::<Result<_>>()?;
        Ok(Some(fields))
    }
}

fn scalar_origin_type(ty: &Ty) -> Result<&'static str> {
    match ty {
        Ty::Integer => Ok("integer"),
        Ty::Bool => Ok("boolean"),
        Ty::Text => Ok("java.lang.String"),
        _ => Err(fail(
            "source scalar origin requires a declared scalar field",
        )),
    }
}
