//! Typed business object identities share the execution store. A borrowed
//! object's scalar content keeps its input origin, while writes to a fresh
//! object are projected only when its content crosses the entry boundary.
use super::*;

#[derive(Clone)]
enum Origin {
    Parameter { path: String, nullable: bool },
    State { binding: u64, path: Vec<String> },
    Allocation,
    EntryReceiver,
}

enum BorrowedRoot<'a> {
    Parameter(&'a str),
    State(u64),
}

fn borrowed_root(origin: &Origin) -> Option<BorrowedRoot<'_>> {
    match origin {
        Origin::Parameter { path, .. } if !path.contains('.') => Some(BorrowedRoot::Parameter(path)),
        Origin::State { binding, path } if path.is_empty() => Some(BorrowedRoot::State(*binding)),
        _ => None,
    }
}

#[derive(Default)]
pub(super) struct SourceObjects {
    objects: BTreeMap<i64, (String, Origin)>,
    states: BTreeMap<(String, String), Expr>,
    entry_receiver: Option<i64>,
    generic_products: BTreeMap<i64, BTreeMap<String, Value>>,
    borrowed_fields: BTreeMap<(i64, String), i64>,
    pub(super) state_bindings: BTreeMap<u64, String>,
    pub(super) compared_parameters: BTreeMap<i64, String>,
    pub(super) compared_states: BTreeMap<i64, u64>,
}
impl SourceObjects {
    pub(super) fn allocation_owner(&self, reference: &Expr) -> Option<&str> {
        let (owner, origin) = self.objects.get(&reference.as_heap_ref()?)?;
        matches!(origin, Origin::Allocation).then_some(owner.as_str())
    }

    pub(super) fn borrowed_parameter(&self, reference: &Expr) -> Option<(&str, &str)> {
        let (owner, Origin::Parameter { path, .. }) = self.objects.get(&reference.as_heap_ref()?)? else {
            return None;
        };
        Some((owner, path))
    }

    // Fresh local allocations cannot equal an identity acquired from the
    // entry snapshot. Private heap IDs alone do not prove this relation.
    fn fresh_and_acquired(&self, left: &Expr, right: &Expr) -> bool {
        let origin = |value: &Expr| {
            value.as_heap_ref().and_then(|id| self.objects.get(&id)).map(|(_, origin)| origin)
        };
        matches!((origin(left), origin(right)),
            (Some(Origin::Allocation), Some(Origin::Parameter { .. } | Origin::State { .. } | Origin::EntryReceiver))
            | (Some(Origin::Parameter { .. } | Origin::State { .. } | Origin::EntryReceiver), Some(Origin::Allocation)))
    }

    fn knows_identity(&self, value: &Expr) -> bool {
        value.is_null() || self.owns(value)
    }

    pub(super) fn has_complete_identity_partition(&self, value: &Expr) -> bool {
        value.as_total_case_when().is_some_and(|(arms, fallback)| {
            arms.iter().all(|(_, identity)| self.knows_identity(identity)) && self.knows_identity(fallback)
        })
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
                (owner.into(), Origin::Parameter { path: name.into(), nullable }),
            );
            Ok(Value::Term(Ty::Ref, b::heap_ref(reference.id())))
        } else {
            symbolic(name, ty, &mut self.heap, Some(declared))
        }
    }

    /// The certified invocation's live This is one acquired identity, not a
    /// new object or a copy of its business state.
    pub(super) fn source_entry_receiver_value(&mut self, owner: &str) -> Value {
        let id = if let Some(id) = self.source_objects.entry_receiver { id } else {
            let id = self.heap.new_path_object().id();
            self.source_objects.objects.insert(id, (owner.into(), Origin::EntryReceiver));
            self.source_objects.entry_receiver = Some(id);
            id
        };
        Value::Term(Ty::Ref, b::heap_ref(id))
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
            return symbolic(&symbol, &ty, &mut self.heap, Some(field.java_type()));
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
            self.state_roots.insert(format!("{owner}.{name}"), reference.id());
            b::heap_ref(reference.id())
        } else {
            let primitive = scalar_origin_type(&ty)?;
            b::param(source_state_symbol(name), primitive)
        };
        self.source_objects.states.insert(key, value.clone());
        Ok(Value::Term(ty, value))
    }

    fn scalar_state_reference(&mut self, owner: &str, name: &str) -> Expr {
        self.acquire_source_state_root(format!("{owner}.{name}"))
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

    pub(super) fn construct_generic_product(
        &mut self, proof: &crate::normalized::generic_product::InstantiatedProduct<'_, '_>, args: Vec<Value>,
    ) -> Result<Value> {
        need(args.len() == proof.arity(), "generic product arity")?;
        let mut fields = BTreeMap::new();
        for &(field, index) in proof.bindings() {
            let value = args[index].scalar()?;
            need(args[index].ty() == Ty::Text || value.is_null(), "generic product argument type")?;
            fields.insert(field.to_owned(), Value::Term(Ty::Text, value));
        }
        let reference = self.heap.new_object();
        self.realtime_local_allocations.insert(reference.id());
        self.source_objects.objects.insert(reference.id(), (proof.name(), Origin::Allocation));
        self.source_objects.generic_products.insert(reference.id(), fields);
        Ok(Value::Term(Ty::Ref, b::heap_ref(reference.id())))
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
        if class.is_record() {
            need(args.len() == class.fields().len(), "native record constructor arity")?;
            for (field, value) in class.fields().iter().zip(args) {
                let ty = self.project.parameter_type(field.java_type())?;
                let scalar = value.scalar()?;
                need(value.ty() == ty || ty == Ty::Text && scalar.is_null(), "native record component type mismatch")?;
                self.realtime_store.store_guarded(path.clone(), r.clone(), field.name(), scalar);
            }
            return Ok(Value::Term(Ty::Ref, r));
        }
        for field in class.fields().iter().filter(|field| !field.is_static()) {
            let default = match self.project.parameter_type(field.java_type())? {
                Ty::Integer => b::lit_int(0),
                Ty::Bool => b::lit_bool(false),
                Ty::Ref | Ty::Text => b::lit_null(),
                _ => return Err(fail("unsupported source field initializer")),
            };
            self.realtime_store
                .store_guarded(path.clone(), r.clone(), field.name(), default);
        }
        // Java initializes every field to its default, then evaluates
        // declaration initializers in source order, then enters the body.
        for field in class.fields().iter().filter(|field| !field.is_static()) {
            if let Some(initializer) = field.initializer() {
                let value = crate::normalized::closed_field_initializer(field.java_type(), initializer)
                    .ok_or_else(|| fail("unsupported source field initializer"))?;
                self.realtime_store.store_guarded(path.clone(), r.clone(), field.name(), value);
            }
        }
        let value = Value::Term(Ty::Ref, r);
        if class.methods().iter().any(|method| method.is_constructor()) {
            let (target, _) = crate::normalized::constructor_for_arity(class, args.len())
                .ok_or_else(|| fail("unresolved source constructor"))?;
            self.java_call_on(
                &target,
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
        if let Some(fields) = self.source_objects.generic_products.get(&id) {
            return fields.get(name).cloned().ok_or_else(|| fail("undeclared generic product field"));
        }
        if matches!(origin, Origin::EntryReceiver) {
            return self.read_entry_state(&owner.clone(), name, path);
        }
        let Some((class, field, child_owner)) = self.program.find_class(owner)
            .and_then(|class| class.fields().iter().find(|field| field.name() == name)
                .and_then(|field| self.project.source_owner(field.java_type()).map(|child| (class, field, child))))
        else { return self.read_guarded_source_scalar(reference, name, path); };
        let nullable = crate::normalized::source_nullable_field(class, field);
        need(!nullable || matches!(origin, Origin::Parameter { .. }),
            "nullable object member requires a borrowed parameter path")?;
        let child = match origin {
            Origin::Allocation => return self.read_source_field_on_path(reference, name, path),
            Origin::EntryReceiver => unreachable!("live receiver dispatched to entry state"),
            Origin::Parameter { path: root, .. } => Origin::Parameter {
                path: format!("{root}.{name}"),
                nullable,
            },
            Origin::State { binding, path } => {
                let mut path = path.clone();
                path.push(name.to_owned());
                Origin::State { binding: *binding, path }
            }
        };
        let key = (id, name.to_owned());
        if let Some(&child_ref) = self.source_objects.borrowed_fields.get(&key) {
            return Ok(Value::Term(Ty::Ref, b::heap_ref(child_ref)));
        }
        let child_ref = self.heap.new_path_object().id();
        self.source_objects.objects.insert(child_ref, (child_owner.to_owned(), child));
        self.source_objects.borrowed_fields.insert(key, child_ref);
        Ok(Value::Term(Ty::Ref, b::heap_ref(child_ref)))
    }

    pub(super) fn source_object_null(&mut self, reference: &Expr) -> Expr {
        let origin = reference.as_heap_ref().and_then(|id| {
            self.source_objects.objects.get(&id).map(|(_, origin)| (id, origin))
        });
        if let Some((id, Origin::Parameter { path, nullable: true })) = origin {
            self.source_objects.compared_parameters.insert(id, path.clone());
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
            .filter(|entry| !b::path_implies(&path, &b::not(entry.guard.clone())))
            .filter_map(|entry| entry.reference.as_heap_ref())
            .filter(|other| self.source_objects.objects.get(other).is_some_and(|(owner, origin)| {
                owner == &self.source_objects.objects[&id].0
                    && matches!(origin, Origin::State { .. } | Origin::Parameter { .. })
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
            Origin::EntryReceiver => return Err(fail("live receiver must acquire declared state")),
            Origin::Allocation => self
                .realtime_store
                .read_on_path(reference, name, path)
                .ok_or_else(|| fail("uninitialized source field"))?,
            Origin::Parameter { path: root, .. } => b::param(format!("{root}.{name}"), scalar_origin_type(&ty)?),
            Origin::State { binding, path } => {
                let primitive = scalar_origin_type(&ty)?;
                let mut field_path = path.clone();
                field_path.push(name.to_owned());
                b::sym_e_path(*binding, field_path, primitive)
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
        let path = self.normal_path(path);
        if path.as_bool() == Some(false) {
            return Ok(());
        }
        if self.source_objects.has_complete_identity_partition(reference) {
            let (arms, fallback) = reference.as_total_case_when().expect("certified total receiver partition");
            // The receiver and RHS have already been evaluated. Dispatch
            // their single store using the existing first-match domains.
            let mut remaining = path;
            for (condition, receiver) in arms {
                let (guard, later) = b::first_match_domains(remaining, condition);
                self.write_source_field_on_identity(receiver, field, value.clone(), guard)?;
                remaining = later;
            }
            return self.write_source_field_on_identity(fallback, field, value, remaining);
        }
        self.write_source_field_on_identity(reference, field, value, path)
    }

    fn write_source_field_on_identity(
        &mut self,
        reference: &Expr,
        field: &str,
        value: Value,
        path: Expr,
    ) -> Result<()> {
        let path = self.normal_path(path);
        if path.as_bool() == Some(false) {
            return Ok(());
        }
        if reference.is_null() {
            self.throw_source_on_path(path, "java.lang.NullPointerException");
            return Ok(());
        }
        let (owner, origin) = reference.as_heap_ref()
            .and_then(|id| self.source_objects.objects.get(&id))
            .ok_or_else(|| fail("field write requires a closed source object"))?;
        if matches!(origin, Origin::EntryReceiver) {
            return self.write_entry_state(&owner.clone(), field, value, path);
        }
        use super::realtime_state::FieldWriteTarget;
        let target = match origin {
            Origin::Allocation => None,
            Origin::State { binding, path } if path.is_empty() => {
                Some(FieldWriteTarget::EntryState(vec![self.source_objects.state_bindings[binding].clone(), field.into()]))
            }
            Origin::Parameter { path, .. } if !path.contains('.') => {
                Some(FieldWriteTarget::Parameter(format!("{path}.{field}")))
            }
            _ => return Err(fail("observable mutation requires a supported borrowed root")),
        };
        let declaration = self
            .program
            .find_class(owner)
            .and_then(|c| c.fields().iter().find(|f| f.name() == field && !f.is_static()))
            .ok_or_else(|| fail("unknown assigned instance field"))?;
        let ty = self.project.parameter_type(declaration.java_type())?;
        need(value.ty() == ty, "field assignment type mismatch")?;
        if target.is_some() {
            scalar_origin_type(&ty)?;
        }

        // Java evaluates receiver and right-hand side before putfield checks
        // the receiver. A failed check records a throw, never a write.
        let null = self.source_object_null(reference);
        self.throw_source_on_path(b::and(path.clone(), null), "java.lang.NullPointerException");
        let path = self.normal_path(path);
        if path.as_bool() == Some(false) { return Ok(()); }
        let value = value.scalar()?;
        match target {
            Some(FieldWriteTarget::EntryState(fields)) =>
                self.realtime_effects.write_state(path.clone(), &fields, value.clone()),
            Some(FieldWriteTarget::Parameter(name)) =>
                self.realtime_effects.write_parameter_field(path.clone(), name, value.clone()),
            None => {}
        }
        self.realtime_store.store_guarded(path, reference.clone(), field, value);
        Ok(())
    }

    pub(super) fn source_reference_equality(&mut self, mut left: Expr, mut right: Expr, different: bool) -> Result<Expr> {
        // Both operands have been evaluated. Ordering the two comparison
        // partitions by their guard keys never changes source evaluation
        // order or the first-match order within either partition.
        let left_partition = self.source_objects.has_complete_identity_partition(&left);
        let right_partition = self.source_objects.has_complete_identity_partition(&right);
        if left_partition && right_partition {
            let names = &self.source_objects.compared_parameters;
            let order = crate::canonical_names::CanonicalNames {
                heap: names, external: None, total_temporals: None, reductions: None, temporals: None,
            };
            let key = |(guard, _): &(Expr, Expr)| math::canonical_order_key(guard, &order);
            let a = left.as_case_when().unwrap().iter().map(key);
            let z = right.as_case_when().unwrap().iter().map(key);
            if a.cmp(z).is_gt() { std::mem::swap(&mut left, &mut right); }
        }
        let selected = match (left.as_total_case_when(), right.as_total_case_when()) {
            (Some(arms), _) if left_partition && (self.source_objects.knows_identity(&right) || right_partition) =>
                Some((arms, &right, true)),
            (None, Some(arms)) if right_partition && self.source_objects.knows_identity(&left) =>
                Some((arms, &left, false)),
            _ => None,
        };
        if let Some(((arms, fallback), other, selected_left)) = selected {
            // A certified partition has only atomic identity arms. Two
            // partitions therefore recurse at most twice, with each pair
            // subject to the ordinary execution-work budget.
            let mut compare = |value: &Expr| {
                self.budget()?;
                let (a, z) = if selected_left { (value, other) } else { (other, value) };
                self.source_reference_equality(a.clone(), z.clone(), different)
            };
            let mut result = Vec::with_capacity(arms.len() + 1);
            for (guard, value) in arms {
                result.push((guard.clone(), compare(value)?));
            }
            return Ok(math::normalize(b::case_when(result, compare(fallback)?)));
        }
        self.compare_reference_roots(left, right, different)
    }

    fn compare_reference_roots(&mut self, mut left: Expr, mut right: Expr, different: bool) -> Result<Expr> {
        let null_peer = if left.is_null() && self.source_objects.owns(&right) {
            Some(&right)
        } else if right.is_null() && self.source_objects.owns(&left) {
            Some(&left)
        } else {
            None
        };
        if let Some(peer) = null_peer {
            let is_null = self.source_object_null(peer);
            return Ok(if different { b::not(is_null) } else { is_null });
        }
        if self.source_objects.fresh_and_acquired(&left, &right) { return Ok(b::lit_bool(different)); }
        self.check_borrowed_identity(&left, &right)?;
        let kind = if different { math::BinOpKind::Ne } else { math::BinOpKind::Eq };
        let names = crate::canonical_names::CanonicalNames {
            heap: &self.source_objects.compared_parameters, external: None, total_temporals: None, reductions: None, temporals: None,
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
        let origin = |value: &Expr| {
            let id = value.as_heap_ref()?;
            self.source_objects.objects.get(&id).map(|(_, origin)| (id, origin))
        };
        let roots = match (origin(a), origin(b)) {
            (Some((_, Origin::Allocation)), _) | (_, Some((_, Origin::Allocation))) | (None, None) =>
                return Ok(()),
            (Some((left_id, left)), Some((right_id, right))) =>
                borrowed_root(left).zip(borrowed_root(right))
                    .map(|(left, right)| [(left_id, left), (right_id, right)]),
            _ => None,
        }.ok_or_else(|| fail("borrowed identity comparison requires an alias contract"))?;
        // A different borrowed ID describes a different acquisition path,
        // not necessarily a different Java object.
        for (id, origin) in roots {
            match origin {
                BorrowedRoot::Parameter(path) => { self.source_objects.compared_parameters.insert(id, path.to_owned()); }
                BorrowedRoot::State(binding) => { self.source_objects.compared_states.insert(id, binding); }
            }
        }
        Ok(())
    }

    fn check_source_content_cycle(
        &self, root: &Expr, id: i64, active: &mut BTreeSet<i64>, completed: &mut BTreeSet<i64>,
    ) -> Result<()> {
        let Some((owner, Origin::Allocation)) = self.source_objects.objects.get(&id)
            else { return Ok(()); };
        if completed.contains(&id) { return Ok(()); }
        if !active.insert(id) {
            return Err(format!("[NORM-CYCLIC-CONTENT-EXIT] kind=ABORT location={} problem=cyclic content at the return boundary",
                self.selected_entry.id));
        }
        let class = self.program.find_class(owner).ok_or("missing object return layout")?;
        for field in class.fields() {
            if self.project.parameter_type(field.java_type())? == Ty::Ref {
                let value = self.read_source_field(root, field.name())?.scalar()?;
                if let Some(id) = value.as_heap_ref() {
                    self.check_source_content_cycle(&value, id, active, completed)?;
                }
            }
        }
        active.remove(&id);
        completed.insert(id);
        Ok(())
    }

    pub(super) fn source_record(
        &mut self,
        declared: &JavaType,
        value: &Value,
    ) -> Result<Option<Vec<(String, Expr)>>> {
        let Some(owner) = self.project.source_owner(declared) else {
            return Ok(None);
        };
        let reference = value.scalar()?;
        let Some(id) = reference.as_heap_ref().filter(|id| self.source_objects.objects.contains_key(id)) else {
            return Ok(None);
        };
        need(
            matches!(&self.realtime_completion, realtime_state::Completion::Return(value) if value == &reference)
                && self.realtime_throws.is_empty(),
            "object projection requires a total normal return of the same object",
        )?;
        self.check_source_content_cycle(&reference, id, &mut BTreeSet::new(), &mut BTreeSet::new())?;
        let class = self
            .program
            .find_class(owner)
            .ok_or_else(|| fail("missing object return layout"))?;
        let fields = class
            .fields()
            .iter()
            .map(|f| {
                let field = self.read_guarded_source_scalar(&reference, f.name(), &b::lit_bool(true))?;
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

fn scalar_origin_type(ty: &Ty) -> Result<ScalarSymbolType> {
    match ty {
        Ty::Integer => Ok(ScalarSymbolType::Integer),
        Ty::Bool => Ok(ScalarSymbolType::Boolean),
        Ty::Text => Ok(ScalarSymbolType::Text),
        _ => Err(fail(
            "source scalar origin requires a declared scalar field",
        )),
    }
}
