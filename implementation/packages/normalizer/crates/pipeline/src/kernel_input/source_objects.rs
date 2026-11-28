//! Typed business object identities share the execution store. A borrowed
//! object's scalar content keeps its input origin, while writes to a fresh
//! object are projected only when its content crosses the entry boundary.
use super::*;

#[derive(Clone)]
enum Origin {
    Parameter {
        path: String,
        nullable: bool,
    },
    State {
        binding: u64,
        path: Vec<String>,
        nullable: bool,
    },
    Allocation,
    // An object acquired before the invocation is never presumed fresh
    // with respect to a borrowed input.
    Assembly,
    EntryReceiver,
}

enum BorrowedRoot<'a> {
    Parameter(&'a str),
    State(u64, &'a [String]),
}

fn borrowed_root(origin: &Origin) -> Option<BorrowedRoot<'_>> {
    match origin {
        Origin::Parameter { path, .. } => Some(BorrowedRoot::Parameter(path)),
        Origin::State { binding, path, .. } => Some(BorrowedRoot::State(*binding, path)),
        _ => None,
    }
}

#[derive(Default)]
pub(super) struct SourceObjects {
    objects: BTreeMap<i64, (String, Origin)>,
    states: BTreeMap<(String, String), Expr>,
    entry_receiver: Option<i64>,
    assembly_roots: BTreeMap<super::source_entry::AssemblyObject, i64>,
    generic_products: BTreeMap<i64, BTreeMap<String, Value>>,
    borrowed_fields: BTreeMap<(i64, String), i64>,
    pub(super) state_bindings: BTreeMap<u64, String>,
    pub(super) compared_parameters: BTreeMap<i64, String>,
    pub(super) compared_states: BTreeMap<i64, (u64, Vec<String>)>,
}
impl SourceObjects {
    pub(super) fn allocation_owner(&self, reference: &Expr) -> Option<&str> {
        let (owner, origin) = self.objects.get(&reference.as_heap_ref()?)?;
        matches!(origin, Origin::Allocation).then_some(owner.as_str())
    }

    pub(super) fn dispatch_owner(&self, reference: &Expr) -> Option<&str> {
        let (owner, origin) = self.objects.get(&reference.as_heap_ref()?)?;
        matches!(origin, Origin::Allocation | Origin::Assembly).then_some(owner.as_str())
    }

    pub(super) fn borrowed_parameter(&self, reference: &Expr) -> Option<(&str, &str)> {
        let (owner, Origin::Parameter { path, .. }) =
            self.objects.get(&reference.as_heap_ref()?)?
        else {
            return None;
        };
        Some((owner, path))
    }

    // Fresh local allocations cannot equal an identity acquired from the
    // entry snapshot. Private heap IDs alone do not prove this relation.
    fn fresh_and_acquired(&self, left: &Expr, right: &Expr) -> bool {
        let origin = |value: &Expr| {
            value
                .as_heap_ref()
                .and_then(|id| self.objects.get(&id))
                .map(|(_, origin)| origin)
        };
        matches!(
            (origin(left), origin(right)),
            (
                Some(Origin::Allocation),
                Some(
                    Origin::Parameter { .. }
                        | Origin::State { .. }
                        | Origin::Assembly
                        | Origin::EntryReceiver
                )
            ) | (
                Some(
                    Origin::Parameter { .. }
                        | Origin::State { .. }
                        | Origin::Assembly
                        | Origin::EntryReceiver
                ),
                Some(Origin::Allocation)
            )
        )
    }

    pub(super) fn heap_binding_keys(&self) -> BTreeMap<i64, math::HeapBindingIdentity> {
        // Some indexed parameter identities are registered in the comparison
        // ledger rather than the nominal-object table.
        let mut bindings: BTreeMap<_, _> = self
            .compared_parameters
            .iter()
            .map(|(&id, path)| {
                (
                    id,
                    math::HeapBindingIdentity::Parameter(
                        path.split('.').map(str::to_owned).collect(),
                    ),
                )
            })
            .collect();
        bindings.extend(self.objects.iter().filter_map(|(&id, (_, origin))| {
            let identity = match origin {
                Origin::Parameter { path, .. } => math::HeapBindingIdentity::Parameter(
                    path.split('.').map(str::to_owned).collect(),
                ),
                Origin::State { binding, path, .. } => {
                    let mut fields = vec![self.state_bindings.get(binding)?.clone()];
                    fields.extend(path.iter().cloned());
                    math::HeapBindingIdentity::EntrySnapshot(fields)
                }
                Origin::Allocation | Origin::Assembly | Origin::EntryReceiver => return None,
            };
            Some((id, identity))
        }));
        bindings
    }

    fn knows_identity(&self, value: &Expr) -> bool {
        value.is_null() || self.owns(value) || Self::is_open_parameter(value)
    }

    fn is_open_parameter(value: &Expr) -> bool {
        matches!(value.as_sym_var(), Some(SymVarOrigin::Parameter { primitive_type, .. })
            if primitive_type == "java_reference")
    }

    pub(super) fn has_complete_identity_partition(&self, value: &Expr) -> bool {
        value.as_total_case_when().is_some_and(|(arms, fallback)| {
            arms.iter()
                .all(|(_, identity)| self.knows_identity(identity))
                && self.knows_identity(fallback)
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
                self.source_relations
                    .record_inputs
                    .insert(name.into(), owner.into());
            }
        }
        if self.project.direct_source && crate::normalized::root_object_type(self.program, declared)
        {
            return Ok(Value::Term(
                Ty::Ref,
                b::param(name, ScalarSymbolType::OpenJavaReference),
            ));
        }
        if let Some(owner) = self.project.source_owner(declared) {
            let reference = self.heap.new_path_object();
            self.source_objects.objects.insert(
                reference.id(),
                (
                    owner.into(),
                    Origin::Parameter {
                        path: name.into(),
                        nullable,
                    },
                ),
            );
            Ok(Value::Term(Ty::Ref, b::heap_ref(reference.id())))
        } else if nullable && *ty == Ty::Text {
            Ok(Value::Term(
                Ty::Text,
                b::param(name, ScalarSymbolType::NullableText),
            ))
        } else {
            let mut value = symbolic(name, ty, &mut self.heap, Some(declared))?;
            if nullable {
                if let Value::Sequence { null, .. } = &mut value {
                    *null = self.nullable_source_array_root(name)?;
                }
            }
            Ok(value)
        }
    }

    /// The certified invocation's live This is one acquired identity, not a
    /// new object or a copy of its business state.
    pub(super) fn source_entry_receiver_value(&mut self, owner: &str) -> Value {
        let id = if let Some(id) = self.source_objects.entry_receiver {
            id
        } else {
            let id = self.heap.new_path_object().id();
            self.source_objects
                .objects
                .insert(id, (owner.into(), Origin::EntryReceiver));
            self.source_objects.entry_receiver = Some(id);
            id
        };
        Value::Term(Ty::Ref, b::heap_ref(id))
    }

    pub(super) fn assembled_dependency(&mut self, owner: &str, name: &str) -> Option<Value> {
        let link = self.source_assembly?.dependency(owner, name)?;
        let id = if let Some(id) = self.source_objects.assembly_roots.get(&link.object) {
            *id
        } else {
            let id = self.heap.new_path_object().id();
            self.source_objects
                .objects
                .insert(id, (link.target_class.clone(), Origin::Assembly));
            self.source_objects
                .assembly_roots
                .insert(link.object.clone(), id);
            id
        };
        Some(Value::Term(Ty::Ref, b::heap_ref(id)))
    }

    pub(super) fn is_entry_state(&self, owner: &str, name: &str) -> bool {
        self.program.find_class(owner).is_some_and(|class| {
            class
                .fields()
                .iter()
                .any(|f| f.name() == name && crate::normalized::entry_state(class, f))
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
            if !self
                .source_objects
                .state_bindings
                .values()
                .any(|root| root == name)
            {
                let binding = self.fresh_binding();
                self.source_objects
                    .state_bindings
                    .insert(binding, name.into());
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
            let reference = self.heap.new_path_object();
            self.source_objects.objects.insert(
                reference.id(),
                (
                    target.into(),
                    Origin::State {
                        binding: bind,
                        path: Vec::new(),
                        nullable: false,
                    },
                ),
            );
            self.state_roots
                .insert(format!("{owner}.{name}"), reference.id());
            b::heap_ref(reference.id())
        } else {
            let primitive = if crate::normalized::nullable_text_field(class, field) {
                ScalarSymbolType::NullableText
            } else {
                scalar_origin_type(&ty)?
            };
            // This is the declared entry snapshot, not an additional P
            // binding disguised under an internal string prefix.
            b::sym_e(bind, primitive)
        };
        self.source_objects.states.insert(key, value.clone());
        Ok(Value::Term(ty, value))
    }

    fn scalar_state_reference(&mut self, owner: &str, name: &str) -> Expr {
        self.acquire_source_state_root(format!("{owner}.{name}"))
    }

    pub(super) fn read_entry_state(
        &mut self,
        owner: &str,
        name: &str,
        path: &Expr,
    ) -> Result<Value> {
        let initial = self.initial_entry_state(owner, name)?;
        if matches!(initial.ty(), Ty::Ref | Ty::Seq(_)) {
            return Ok(initial);
        }
        let root = self.scalar_state_reference(owner, name);
        let value = self
            .realtime_store
            .read_with_initial(&root, name, path, initial.scalar()?);
        Ok(Value::Term(initial.ty(), value))
    }

    pub(super) fn write_entry_state(
        &mut self,
        owner: &str,
        name: &str,
        value: Value,
        path: Expr,
    ) -> Result<()> {
        let initial = self.initial_entry_state(owner, name)?;
        let scalar = value.scalar()?;
        let nullable_text = matches!(initial.scalar()?.as_sym_var(),
            Some(SymVarOrigin::External { primitive_type, .. }) if primitive_type == "text?");
        need(
            initial.ty() != Ty::Ref
                && (value.ty() == initial.ty() || (nullable_text && scalar.is_null())),
            "entry scalar state assignment type mismatch",
        )?;
        need(
            !scalar.is_null() || nullable_text,
            "nonnullable entry state cannot store a null value",
        )?;
        let value = scalar;
        let guard = self.normal_path(path);
        let root = self.scalar_state_reference(owner, name);
        self.realtime_store
            .store_guarded(guard.clone(), root, name, value.clone());
        self.realtime_effects
            .write_state(guard, &[name.into()], value);
        Ok(())
    }

    pub(super) fn construct_generic_product(
        &mut self,
        proof: &crate::normalized::generic_product::InstantiatedProduct<'_, '_>,
        args: Vec<Value>,
    ) -> Result<Value> {
        need(args.len() == proof.arity(), "generic product arity")?;
        let mut fields = BTreeMap::new();
        for &(field, index) in proof.bindings() {
            let value = args[index].scalar()?;
            need(
                args[index].ty() == Ty::Text || value.is_null(),
                "generic product argument type",
            )?;
            fields.insert(field.to_owned(), Value::Term(Ty::Text, value));
        }
        let reference = self.heap.new_object();
        self.realtime_local_allocations.insert(reference.id());
        self.source_objects
            .objects
            .insert(reference.id(), (proof.name(), Origin::Allocation));
        self.source_objects
            .generic_products
            .insert(reference.id(), fields);
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
            need(
                args.len() == class.fields().len(),
                "native record constructor arity",
            )?;
            for (field, value) in class.fields().iter().zip(args) {
                let ty = self.project.parameter_type(field.java_type())?;
                let scalar = value.scalar()?;
                need(
                    value.ty() == ty || ty == Ty::Text && scalar.is_null(),
                    "native record component type mismatch",
                )?;
                self.realtime_store
                    .store_guarded(path.clone(), r.clone(), field.name(), scalar);
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
                let value =
                    crate::normalized::closed_field_initializer(field.java_type(), initializer)
                        .ok_or_else(|| fail("unsupported source field initializer"))?;
                self.realtime_store
                    .store_guarded(path.clone(), r.clone(), field.name(), value);
            }
        }
        let value = Value::Term(Ty::Ref, r);
        if class.methods().iter().any(|method| method.is_constructor()) {
            let (target, _) = crate::normalized::constructor_for_arity(class, args.len())
                .ok_or_else(|| fail("unresolved source constructor"))?;
            self.java_call_on(&target, args, path, Some(value.clone()))?;
        } else {
            need(args.is_empty(), "implicit constructor accepts no arguments")?;
        }
        Ok(value)
    }

    // A borrowed child keeps all of its original P/E path. Give it an
    // internal identity so a local alias still denotes the same path.
    pub(super) fn read_or_bind_source_field(
        &mut self,
        reference: &Expr,
        name: &str,
        path: &Expr,
    ) -> Result<Value> {
        let (id, owner, origin) = reference
            .as_heap_ref()
            .and_then(|id| {
                self.source_objects
                    .objects
                    .get(&id)
                    .map(|(owner, origin)| (id, owner, origin))
            })
            .ok_or_else(|| fail("unknown source receiver"))?;
        if let Some(fields) = self.source_objects.generic_products.get(&id) {
            return fields
                .get(name)
                .cloned()
                .ok_or_else(|| fail("undeclared generic product field"));
        }
        if matches!(origin, Origin::EntryReceiver) {
            let owner = owner.clone();
            if let Some(value) = self.assembled_dependency(&owner, name) {
                return Ok(value);
            }
            return self.read_entry_state(&owner, name, path);
        }
        let Some((class, field, child_owner)) = self.program.find_class(owner).and_then(|class| {
            class
                .fields()
                .iter()
                .find(|field| field.name() == name)
                .and_then(|field| {
                    self.project
                        .source_owner(field.java_type())
                        .map(|child| (class, field, child))
                })
        }) else {
            return self.read_guarded_source_scalar(reference, name, path);
        };
        let nullable = crate::normalized::source_nullable_field(class, field);
        let child = match origin {
            Origin::Allocation => return self.read_source_field_on_path(reference, name, path),
            Origin::Assembly => return Err(fail("statically assembled dependency is stateless")),
            Origin::EntryReceiver => unreachable!("live receiver dispatched to entry state"),
            Origin::Parameter { path: root, .. } => Origin::Parameter {
                path: format!("{root}.{name}"),
                nullable,
            },
            Origin::State { binding, path, .. } => {
                let mut path = path.clone();
                path.push(name.to_owned());
                Origin::State {
                    binding: *binding,
                    path,
                    nullable,
                }
            }
        };
        let key = (id, name.to_owned());
        if let Some(&child_ref) = self.source_objects.borrowed_fields.get(&key) {
            return Ok(Value::Term(Ty::Ref, b::heap_ref(child_ref)));
        }
        let child_ref = self.heap.new_path_object().id();
        self.source_objects
            .objects
            .insert(child_ref, (child_owner.to_owned(), child));
        self.source_objects.borrowed_fields.insert(key, child_ref);
        Ok(Value::Term(Ty::Ref, b::heap_ref(child_ref)))
    }

    pub(super) fn read_source_field_partition(
        &mut self,
        reference: &Expr,
        field: &str,
        ty: Ty,
        path: Expr,
    ) -> Result<Value> {
        let (arms, fallback) = if reference.is_null() {
            (&[][..], reference)
        } else {
            reference
                .as_total_case_when()
                .expect("certified total receiver partition")
        };
        let mut remaining = path;
        let mut values = Vec::with_capacity(arms.len() + 1);
        let else_guard = b::lit_bool(true);
        for (condition, identity) in arms
            .iter()
            .map(|(c, r)| (c, r))
            .chain(std::iter::once((&else_guard, fallback)))
        {
            let (guard, later) = b::first_match_domains(remaining, condition);
            remaining = later;
            let guard = self.normal_path(guard);
            if guard.as_bool() == Some(false) {
                continue;
            }

            let null = if identity.is_null() {
                b::lit_bool(true)
            } else {
                self.source_object_null(identity)
            };
            self.throw_source_on_path(
                b::and(guard.clone(), null),
                "java.lang.NullPointerException",
            );
            let live = self.normal_path(guard);
            if live.as_bool() == Some(false) {
                continue;
            }
            let value = self.read_or_bind_source_field(identity, field, &live)?;
            need(value.ty() == ty, "selected source field type mismatch")?;
            values.push((live, value.scalar()?));
        }
        // The remaining alternatives cover the normal completion domain.
        // With no live read the typed carrier is never observed.
        let result = match values.pop() {
            Some((_, fallback)) => math::normalize(b::case_when(values, fallback)),
            None => b::lit_null(),
        };
        Ok(Value::Term(ty, result))
    }

    pub(super) fn source_object_null(&mut self, reference: &Expr) -> Expr {
        let origin = reference.as_heap_ref().and_then(|id| {
            self.source_objects
                .objects
                .get(&id)
                .map(|(_, origin)| (id, origin))
        });
        match origin {
            Some((
                id,
                Origin::Parameter {
                    path,
                    nullable: true,
                },
            )) => {
                self.source_objects
                    .compared_parameters
                    .insert(id, path.clone());
            }
            Some((
                id,
                Origin::State {
                    binding,
                    path,
                    nullable: true,
                },
            )) => {
                self.source_objects
                    .compared_states
                    .insert(id, (*binding, path.clone()));
            }
            _ => return b::lit_bool(false),
        }
        b::eq(reference.clone(), b::lit_null())
    }

    fn read_guarded_source_scalar(
        &mut self,
        reference: &Expr,
        field: &str,
        path: &Expr,
    ) -> Result<Value> {
        let id = reference
            .as_heap_ref()
            .ok_or_else(|| fail("unresolved source receiver"))?;
        if matches!(self.source_objects.objects[&id].1, Origin::Allocation) {
            return self.read_source_field_on_path(reference, field, path);
        }
        let initial = self.read_source_field_on_path(reference, field, path)?;
        let path = self.normal_path(path.clone());
        let mut identities = BTreeMap::new();
        let candidates = self
            .realtime_store
            .entries()
            .iter()
            .filter(|entry| entry.field == field && entry.reference != *reference)
            .filter(|entry| !b::path_implies(&path, &b::not(entry.guard.clone())))
            .filter_map(|entry| entry.reference.as_heap_ref())
            .filter(|other| {
                self.source_objects
                    .objects
                    .get(other)
                    .is_some_and(|(owner, origin)| {
                        owner == &self.source_objects.objects[&id].0
                            && matches!(origin, Origin::State { .. } | Origin::Parameter { .. })
                    })
            })
            .collect::<BTreeSet<_>>();
        for other in candidates {
            let candidate = b::heap_ref(other);
            self.check_borrowed_identity(reference, &candidate)?;
            identities.insert(other, b::eq(reference.clone(), candidate));
        }
        let value = self.realtime_store.read_matching_with_initial(
            field,
            &path,
            initial.scalar()?,
            |candidate| {
                if candidate == reference {
                    Some(b::lit_bool(true))
                } else {
                    candidate
                        .as_heap_ref()
                        .and_then(|id| identities.get(&id).cloned())
                }
            },
        );
        Ok(Value::Term(initial.ty(), value))
    }

    pub(super) fn read_source_field_on_path(
        &self,
        reference: &Expr,
        name: &str,
        path: &Expr,
    ) -> Result<Value> {
        let (owner, origin) = self
            .source_objects
            .objects
            .get(
                &reference
                    .as_heap_ref()
                    .ok_or_else(|| fail("unresolved source receiver"))?,
            )
            .ok_or_else(|| fail("unknown source receiver"))?;
        let class = self
            .program
            .find_class(owner)
            .ok_or_else(|| fail("unknown source class"))?;
        let field = class
            .fields()
            .iter()
            .find(|f| f.name() == name)
            .ok_or_else(|| fail("unknown source field"))?;
        let ty = self.project.parameter_type(field.java_type())?;
        let primitive = || {
            if crate::normalized::nullable_text_field(class, field) {
                Ok(ScalarSymbolType::NullableText)
            } else {
                scalar_origin_type(&ty)
            }
        };
        let expr = match origin {
            Origin::Assembly => return Err(fail("statically assembled dependency is stateless")),
            Origin::EntryReceiver => return Err(fail("live receiver must acquire declared state")),
            Origin::Allocation => self
                .realtime_store
                .read_on_path(reference, name, path)
                .ok_or_else(|| fail("uninitialized source field"))?,
            Origin::Parameter { path: root, .. } => {
                b::param(format!("{root}.{name}"), primitive()?)
            }
            Origin::State { binding, path, .. } => {
                let primitive = primitive()?;
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
        if self
            .source_objects
            .has_complete_identity_partition(reference)
        {
            let (arms, fallback) = reference
                .as_total_case_when()
                .expect("certified total receiver partition");
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
        let (owner, origin) = reference
            .as_heap_ref()
            .and_then(|id| self.source_objects.objects.get(&id))
            .ok_or_else(|| fail("field write requires a closed source object"))?;
        if matches!(origin, Origin::EntryReceiver) {
            return self.write_entry_state(&owner.clone(), field, value, path);
        }
        use super::realtime_state::FieldWriteTarget;
        let target = match origin {
            Origin::Allocation => None,
            Origin::State { binding, path, .. } => {
                let mut target = vec![self.source_objects.state_bindings[binding].clone()];
                target.extend(path.iter().cloned());
                target.push(field.into());
                Some(FieldWriteTarget::EntryState(target))
            }
            Origin::Parameter { path, .. } => {
                Some(FieldWriteTarget::Parameter(format!("{path}.{field}")))
            }
            Origin::Assembly => {
                return Err(fail(
                    "statically assembled dependency is not a writable state root",
                ))
            }
            Origin::EntryReceiver => unreachable!("entry receiver dispatched to declared state"),
        };
        let declaration = self
            .program
            .find_class(owner)
            .and_then(|c| {
                c.fields()
                    .iter()
                    .find(|f| f.name() == field && !f.is_static())
            })
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
        if path.as_bool() == Some(false) {
            return Ok(());
        }
        let value = value.scalar()?;
        match target {
            Some(FieldWriteTarget::EntryState(fields)) => {
                self.realtime_effects
                    .write_state(path.clone(), &fields, value.clone())
            }
            Some(FieldWriteTarget::Parameter(name)) => {
                self.realtime_effects
                    .write_parameter_field(path.clone(), name, value.clone())
            }
            None => {}
        }
        self.realtime_store
            .store_guarded(path, reference.clone(), field, value);
        Ok(())
    }

    pub(super) fn source_reference_equality(
        &mut self,
        mut left: Expr,
        mut right: Expr,
        different: bool,
    ) -> Result<Expr> {
        // Both operands have been evaluated. Ordering the two comparison
        // partitions by their guard keys never changes source evaluation
        // order or the first-match order within either partition.
        let left_partition = self.source_objects.has_complete_identity_partition(&left);
        let right_partition = self.source_objects.has_complete_identity_partition(&right);
        if left_partition && right_partition {
            let order = self.execution_order(true);
            let key = |(guard, _): &(Expr, Expr)| math::canonical_order_key(guard, &order);
            let a = left.as_case_when().unwrap().iter().map(key);
            let z = right.as_case_when().unwrap().iter().map(key);
            if a.cmp(z).is_gt() {
                std::mem::swap(&mut left, &mut right);
            }
        }
        let selected = match (left.as_total_case_when(), right.as_total_case_when()) {
            (Some(arms), _)
                if left_partition
                    && (self.source_objects.knows_identity(&right) || right_partition) =>
            {
                Some((arms, &right, true))
            }
            (None, Some(arms)) if right_partition && self.source_objects.knows_identity(&left) => {
                Some((arms, &left, false))
            }
            _ => None,
        };
        if let Some(((arms, fallback), other, selected_left)) = selected {
            // A certified partition has only atomic identity arms. Two
            // partitions therefore recurse at most twice, with each pair
            // subject to the ordinary execution-work budget.
            let mut compare = |value: &Expr| {
                self.budget()?;
                let (a, z) = if selected_left {
                    (value, other)
                } else {
                    (other, value)
                };
                self.source_reference_equality(a.clone(), z.clone(), different)
            };
            let mut result = Vec::with_capacity(arms.len() + 1);
            for (guard, value) in arms {
                result.push((guard.clone(), compare(value)?));
            }
            let fallback = compare(fallback)?;
            return Ok(self.normalize_execution_value(b::case_when(result, fallback)));
        }
        self.compare_reference_roots(left, right, different)
    }

    fn compare_reference_roots(
        &mut self,
        mut left: Expr,
        mut right: Expr,
        different: bool,
    ) -> Result<Expr> {
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
        // Both dispatch owners here denote actual source allocations, either
        // in this invocation or at statically proved assembly sites. This
        // says nothing about aliasing with an arbitrary borrowed input.
        if self.source_objects.dispatch_owner(&left).is_some()
            && self.source_objects.dispatch_owner(&right).is_some()
        {
            return Ok(b::lit_bool((left == right) != different));
        }
        if self.source_objects.fresh_and_acquired(&left, &right)
            || (SourceObjects::is_open_parameter(&left)
                && self.source_objects.allocation_owner(&right).is_some())
            || (SourceObjects::is_open_parameter(&right)
                && self.source_objects.allocation_owner(&left).is_some())
        {
            return Ok(b::lit_bool(different));
        }
        self.check_borrowed_identity(&left, &right)?;
        let kind = if different {
            math::BinOpKind::Ne
        } else {
            math::BinOpKind::Eq
        };
        let names = self.execution_order(true);
        if math::canonical_order_ordered_comparison(kind, &left, &right, &names).is_some() {
            std::mem::swap(&mut left, &mut right);
        }
        Ok(if different {
            b::ne(left, right)
        } else {
            b::eq(left, right)
        })
    }

    pub(super) fn check_borrowed_identity(&mut self, a: &Expr, b: &Expr) -> Result<()> {
        if a == b {
            return Ok(());
        }
        let origin = |value: &Expr| {
            let id = value.as_heap_ref()?;
            self.source_objects
                .objects
                .get(&id)
                .map(|(_, origin)| (id, origin))
        };
        let roots = match (origin(a), origin(b)) {
            (Some((_, Origin::Allocation)), _)
            | (_, Some((_, Origin::Allocation)))
            | (None, None) => return Ok(()),
            (Some((left_id, left)), Some((right_id, right))) => borrowed_root(left)
                .zip(borrowed_root(right))
                .map(|(left, right)| vec![(left_id, left), (right_id, right)]),
            // A wide Java parameter may denote this nominal input object.
            // Bind the typed acquisition and leave the alias test symbolic.
            (Some((id, origin)), None) if SourceObjects::is_open_parameter(b) => {
                borrowed_root(origin).map(|root| vec![(id, root)])
            }
            (None, Some((id, origin))) if SourceObjects::is_open_parameter(a) => {
                borrowed_root(origin).map(|root| vec![(id, root)])
            }
            _ => None,
        }
        .ok_or_else(|| fail("borrowed identity comparison requires an alias contract"))?;
        // A different borrowed ID describes a different acquisition path,
        // not necessarily a different Java object.
        for (id, origin) in roots {
            match origin {
                BorrowedRoot::Parameter(path) => {
                    self.source_objects
                        .compared_parameters
                        .insert(id, path.to_owned());
                }
                BorrowedRoot::State(binding, path) => {
                    self.source_objects
                        .compared_states
                        .insert(id, (binding, path.to_vec()));
                }
            }
        }
        Ok(())
    }

    fn check_source_content_cycle(
        &self,
        root: &Expr,
        id: i64,
        path: &Expr,
        active: &mut BTreeSet<i64>,
        completed: &mut BTreeSet<i64>,
    ) -> Result<()> {
        let Some((owner, Origin::Allocation)) = self.source_objects.objects.get(&id) else {
            return Ok(());
        };
        if completed.contains(&id) {
            return Ok(());
        }
        if !active.insert(id) {
            return Err(format!("[NORM-CYCLIC-CONTENT-EXIT] kind=ABORT location={} problem=cyclic content at the return boundary",
                self.selected_entry.id));
        }
        let class = self
            .program
            .find_class(owner)
            .ok_or("missing object return layout")?;
        for field in class.fields() {
            if self.project.parameter_type(field.java_type())? == Ty::Ref {
                let value = self
                    .read_source_field_on_path(root, field.name(), path)?
                    .scalar()?;
                if let Some(id) = value.as_heap_ref() {
                    self.check_source_content_cycle(&value, id, path, active, completed)?;
                }
            }
        }
        active.remove(&id);
        completed.insert(id);
        Ok(())
    }

    pub(super) fn freeze_source_port_record(
        &mut self,
        reference: Expr,
        declared: &JavaType,
        fields: &BTreeMap<String, Ty>,
        path: &Expr,
    ) -> Result<Expr> {
        let expected = self
            .project
            .source_owner(declared)
            .ok_or_else(|| fail("port data record has no source declaration"))?;
        let id = reference
            .as_heap_ref()
            .ok_or_else(|| fail("port data record requires its acquired identity"))?;
        need(
            self.source_objects
                .objects
                .get(&id)
                .is_some_and(|(owner, _)| owner == expected),
            "port data record does not have the declared source type",
        )?;
        let null = self.source_object_null(&reference);
        need(
            b::path_implies(path, &b::not(null)),
            "port data record must be acquired and non-null",
        )?;
        // The declared by-value argument is captured at the invocation,
        // after evaluating all actual arguments. Nothing retains a live
        // object pointer that later mutations could change.
        let mut snapshot = Vec::with_capacity(fields.len());
        for (name, expected) in fields {
            let value = self.read_or_bind_source_field(&reference, name, path)?;
            need(
                value.ty() == *expected,
                "port data record field type mismatch",
            )?;
            snapshot.push(value.scalar()?);
        }
        Ok(b::array_lit(snapshot))
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
        let Some(id) = reference
            .as_heap_ref()
            .filter(|id| self.source_objects.objects.contains_key(id))
        else {
            return Ok(None);
        };
        let terminal = self
            .realtime_completion
            .terminal_alternatives()
            .ok_or_else(|| fail("object projection requires a complete entry outcome"))?;
        need(
            terminal.same_normal_return_value() == Some(&reference),
            "object projection requires the same object on every normal return",
        )?;
        let mut normal = b::lit_bool(false);
        for item in terminal.as_slice() {
            if matches!(item.outcome, realtime_state::TerminalOutcome::Return(_)) {
                normal = b::or(normal, item.guard.clone());
            }
        }
        let normal = math::normalize(normal);
        need(
            normal.as_bool() != Some(false),
            "object projection requires a normal return",
        )?;
        // Returned content exists on the normal domain only. An exceptional
        // construction never supplies default or partially initialized fields.
        self.check_source_content_cycle(
            &reference,
            id,
            &normal,
            &mut BTreeSet::new(),
            &mut BTreeSet::new(),
        )?;
        let class = self
            .program
            .find_class(owner)
            .ok_or_else(|| fail("missing object return layout"))?;
        let fields = class
            .fields()
            .iter()
            .map(|f| {
                let field = self.read_guarded_source_scalar(&reference, f.name(), &normal)?;
                let scalar = field.scalar()?;
                if field.ty() == Ty::Text && !crate::normalized::nullable_text_field(class, f) {
                    need(
                        !scalar.is_null(),
                        "nonnullable record text field was not initialized",
                    )?;
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
