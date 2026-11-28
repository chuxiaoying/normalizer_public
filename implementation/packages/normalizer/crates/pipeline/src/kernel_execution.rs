use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Arithmetic {
    Add,
    Sub,
    Mul,
}

impl Arithmetic {
    fn build(self, left: Expr, right: Expr) -> Expr {
        match self {
            Self::Add => b::add(left, right),
            Self::Sub => b::sub(left, right),
            Self::Mul => b::mul(left, right),
        }
    }
}

// AIR comes from a directive, independently of the Java declaration.
fn preceding_uninitialized<'a>(body: &'a [S], index: usize, binding: &str) -> Option<&'a JavaType> {
    match body.get(index.checked_sub(1)?) {
        Some(S::VarDecl {
            name,
            java_type,
            initializer: None,
            ..
        }) if name == binding => Some(java_type),
        _ => None,
    }
}

enum LocalIrInitializer {
    TextMap,
    NullableText,
    MemberChoice,
    Integer,
    Relation,
    Decimal,
}

impl LocalIrInitializer {
    fn for_binding(stmt: &S, expected: &str) -> Option<Self> {
        use normalizer_ir::local::LocalIr;
        let S::LocalIr(operation) = stmt else {
            return None;
        };
        let (binding, kind) = match operation {
            LocalIr::LocalMapEmpty { binding } => (binding.as_str(), Self::TextMap),
            LocalIr::LocalMapGet { binding, .. } => (binding.as_str(), Self::NullableText),
            LocalIr::MemberChoice { binding, .. } => (binding.as_str(), Self::MemberChoice),
            LocalIr::ProjectedUnionCount { binding, .. } => (binding.as_str(), Self::Integer),
            LocalIr::IntegerRange(range) if !range.accumulate => {
                (range.binding.as_str(), Self::Integer)
            }
            LocalIr::SqlWrite(write) => (write.binding.as_str(), Self::Integer),
            LocalIr::Relation(query) if !query.accumulate => {
                (query.binding.as_str(), Self::Relation)
            }
            LocalIr::Decimal(decimal) => (decimal.binding.as_str(), Self::Decimal),
            _ => return None,
        };
        (binding == expected).then_some(kind)
    }
}

#[derive(Clone, Copy)]
pub(super) struct SelectedEntry<'a> {
    pub(super) id: &'a str,
    pub(super) syntax: &'a simulator_parser::JavaMethod,
}

pub(super) struct Engine<'a> {
    pub(super) program: &'a JavaProgram,
    pub(super) temporal_definitions: Vec<super::temporal_graph::Definition>,
    pub(super) source_relations: super::business_relation::Relations,
    pub(super) source_objects: super::source_objects::SourceObjects,
    pub(super) source_array_writes: BTreeMap<String, BTreeSet<i64>>,
    // Observability counters; normalized expression depth is bounded at the
    // only deserialization boundary, and Java calls have their own active stack.
    pub(super) node_depth: usize,
    pub(super) max_node_depth: usize,
    pub(super) project: &'a Project,
    pub(super) methods: BTreeMap<String, AdmittedMethod<'a>>,
    pub(super) active: BTreeSet<String>,
    pub(super) steps: usize,
    pub(super) selected_entry: SelectedEntry<'a>,
    pub(super) public_source_entry: Option<&'a str>,
    pub(super) source_root: Option<&'a Path>,
    pub(super) source_assembly: Option<&'a super::SourceReceiver>,
    pub(super) active_static_receiver: Option<(String, String)>,
    pub(super) entry_context: Option<&'a business_entry::EntryContext>,
    pub(super) state_roots: BTreeMap<String, i64>,
    pub(super) checked_ports: BTreeMap<String, actions::CheckedPort>,
    pub(super) pending_completions: BTreeMap<u64, std::sync::Arc<port_completion::PortSpec>>,
    pub(super) caught_exceptions: Option<Vec<realtime_state::CompletionAlternative>>,
    pub(super) next_binding: u64,
    /// Heap identities allocated during this execution. The direct renderer
    /// may hide stores for these identities only under a closed local-state
    /// contract.
    pub(super) realtime_local_allocations: BTreeSet<i64>,
    pub(super) realtime_store: realtime_state::SymbolicStore,
    pub(super) realtime_effects: realtime_state::EffectSequence,
    pub(super) realtime_completion: realtime_state::Completion,
    /// Path-guarded exception conditions produced at their execution point.
    pub(super) realtime_throws: Vec<Expr>,
    /// Whether every control-flow boundary in the realtime slice is certified.
    pub(super) heap: Heap,
    pub(super) action_path: Expr,
    pub(super) semantic_calls: usize,
    pub(super) java_calls: usize,
    pub(super) node_visits: usize,
    pub(super) branch_splits: usize,
    pub(super) branch_collapses: usize,
    pub(super) max_active_calls: usize,
}

// Execution and publication share stable identities, not display handles.
pub(super) struct ExecutionOrder<'a> {
    definitions: &'a [super::temporal_graph::Definition],
    captured: BTreeMap<String, String>,
    observations: BTreeMap<u64, math::ObservationIdentity>,
    heap_bindings: BTreeMap<i64, math::HeapBindingIdentity>,
    effects: &'a realtime_state::EffectSequence,
}

impl math::SortContext for ExecutionOrder<'_> {
    fn captured_parameters(&self) -> Option<&BTreeMap<String, String>> {
        (!self.captured.is_empty()).then_some(&self.captured)
    }
    fn heap_binding_identity(&self, id: i64) -> Option<&math::HeapBindingIdentity> {
        self.heap_bindings.get(&id)
    }

    fn observation_identities(&self) -> Option<&BTreeMap<u64, math::ObservationIdentity>> {
        (!self.observations.is_empty()).then_some(&self.observations)
    }

    fn total_failure_classes(&self, receipt: u64) -> Option<Vec<&str>> {
        self.effects.total_failure_classes(receipt)
    }

    fn is_total_receipt_value(&self, binding: u64) -> bool {
        self.effects.events().iter().any(|event| event.guard.as_bool() == Some(true)
            && matches!(&event.operation, super::realtime_state::EffectOperation::Port { receipt, completion, .. }
                if receipt.binding_id() == binding && !completion.throwing()))
    }
}

impl<'a> Engine<'a> {
    pub(super) fn execution_order(&self, include_heap: bool) -> ExecutionOrder<'_> {
        let mut observations = self
            .source_objects
            .state_bindings
            .iter()
            .map(|(&id, root)| {
                (
                    id,
                    math::ObservationIdentity::EntrySnapshot(vec![root.clone()]),
                )
            })
            .collect::<BTreeMap<_, _>>();
        observations.extend(self.realtime_effects.observation_identities());
        let heap_bindings = if include_heap {
            self.source_objects.heap_binding_keys()
        } else {
            BTreeMap::new()
        };
        let captured = self.entry_context.map_or_else(
            BTreeMap::new,
            business_entry::EntryContext::captured_parameters,
        );
        ExecutionOrder {
            definitions: &self.temporal_definitions,
            captured,
            observations,
            heap_bindings,
            effects: &self.realtime_effects,
        }
    }

    pub(super) fn normalize_execution_value(&self, value: Expr) -> Expr {
        let order = self.execution_order(value.has_heap_ref());
        if order.definitions.is_empty()
            && order.captured.is_empty()
            && order.observations.is_empty()
            && order.heap_bindings.is_empty()
        {
            math::normalize(value)
        } else {
            math::normalize_with(value, &order)
        }
    }

    fn order_execution_comparison(&self, op: BinOpKind, left: Expr, right: Expr) -> Expr {
        let order = self.execution_order(left.has_heap_ref() || right.has_heap_ref());
        if let Some((reverse, true)) =
            math::canonical_order_ordered_comparison(op, &left, &right, &order)
        {
            b::binop_public(reverse, right, left)
        } else {
            b::binop_public(op, left, right)
        }
    }

    fn observe_clock(&mut self, path: Expr) -> Value {
        let guard = self.normal_path(path);
        if guard.as_bool() == Some(false) {
            return Value::Term(Ty::Integer, b::lit_null());
        }
        let binding = self.fresh_binding();
        self.realtime_effects
            .acquire(guard, realtime_state::Acquisition::Clock, binding);
        Value::Term(Ty::Integer, b::sym_e(binding, ScalarSymbolType::Integer))
    }

    fn diagnostic_entry(&self) -> &str {
        self.public_source_entry.unwrap_or(self.selected_entry.id)
    }
    fn node_arithmetic(
        &mut self,
        op: Arithmetic,
        left: &Node,
        right: &Node,
        env: &mut Env,
        path: Expr,
    ) -> Result<Value> {
        let left = self.node(left, env, path.clone())?;
        let right = self.node(right, env, path)?;
        need(
            left.ty() == Ty::Integer && right.ty() == Ty::Integer,
            "exact arithmetic requires business numbers",
        )?;
        Ok(Value::Term(
            Ty::Integer,
            self.normalize_execution_value(op.build(left.scalar()?, right.scalar()?)),
        ))
    }

    fn node_inner(&mut self, n: &Node, env: &mut Env, path: Expr) -> Result<Value> {
        self.budget()?;
        self.node_visits += 1;
        let path = self.normal_path(path);
        if crate::profile::selected("kernel_execution.node") {
            crate::profile::event(
                "kernel_execution.node",
                &format!(
                    "step={} node={:?} path={}",
                    self.steps,
                    n,
                    crate::format_expr_sql(&path)
                ),
            );
        }
        match n {
            Node::ReadClock => Ok(self.observe_clock(path)),
            Node::ReadState { path: fields } => self.read_adapter_state(fields, &path),
            Node::ObserveTuple(observation) => {
                self.observe_tuple(&observation.tuple, &observation.body, env, path)
            }
            Node::OrderedTextMap { entries } => {
                let acquired = entries.iter().map(|entry| {
                    let value = match &entry.source {
                        OrderedMapSource::Literal { text } => text.to_expression(),
                        OrderedMapSource::Parameter { name } => {
                            let value = env.get(name).ok_or_else(|| fail("unbound readonly Map entry"))?;
                            need(value.ty() == Ty::Text, "readonly Map entry requires an acquired nullable text parameter")?;
                            value.scalar()?
                        }
                    };
                    Ok((entry.key.to_expression(), value))
                }).collect::<Result<Vec<_>>>()?;
                let map = math::domains::composite::lifted_ordered_text_map(&acquired)?;
                Ok(Value::Term(Ty::OrderedTextMap, map))
            }
            Node::TextEqual { left, right } => {
                let left = self.node(left, env, path.clone())?;
                let right = self.node(right, env, path)?;
                let a = left.scalar()?;
                let z = right.scalar()?;
                need(
                    (left.ty() == Ty::Text || a.is_null())
                        && (right.ty() == Ty::Text || z.is_null()),
                    "native text comparison requires text or null",
                )?;
                Ok(Value::Term(
                    Ty::Bool,
                    self.order_execution_comparison(BinOpKind::Eq, a, z),
                ))
            }
            Node::TextConcat { left, right } => {
                let left = self.node(left, env, path.clone())?;
                let right = self.node(right, env, path)?;
                let a = left.scalar()?;
                let z = right.scalar()?;
                need(
                    left.ty() == Ty::Text && right.ty() == Ty::Text && !a.is_null() && !z.is_null(),
                    "native text concatenation requires two non-null text values",
                )?;
                Ok(Value::Term(
                    Ty::Text,
                    self.normalize_execution_value(b::add(a, z)),
                ))
            }
            Node::TrimText { value } => {
                let text = self.node(value, env, path)?;
                need(
                    text.ty() == Ty::Text || text.scalar().is_ok_and(|value| value.is_null()),
                    "standard trim requires a nullable text operand",
                )?;
                Ok(Value::Term(
                    Ty::Text,
                    crate::normalized_text::trim(text.scalar()?),
                ))
            }
            Node::DefaultTextIfBlank { value, fallback } => {
                let text = self.node(value, env, path.clone())?;
                let fallback = self.node(fallback, env, path)?;
                need(
                    (text.ty() == Ty::Text || text.scalar().is_ok_and(|value| value.is_null()))
                        && fallback.ty() == Ty::Text,
                    "standard defaultIfBlank requires text operands",
                )?;
                Ok(Value::Term(
                    Ty::Text,
                    crate::normalized_text::default_if_blank(text.scalar()?, fallback.scalar()?),
                ))
            }
            Node::ThrowUnknownInput { name } => {
                need(
                    path.as_bool() == Some(true),
                    "unconditional throw must own the entry domain",
                )?;
                let value = env
                    .get(name)
                    .filter(|value| value.ty() == Ty::UnknownInput)
                    .ok_or_else(|| fail("native throw requires the exact unknown input"))?
                    .scalar()?;
                self.realtime_completion = realtime_state::Completion::ThrowValue(value);
                Ok(Value::Term(Ty::Unit, b::lit_null()))
            }
            Node::NativeTextFlow { flow } => {
                need(
                    path.as_bool() == Some(true),
                    "native text flow must own the complete entry domain",
                )?;
                let completion = self.source_text_flow(flow, env, env, path)?;
                let value = completion.normal_return_value().unwrap_or_else(b::lit_null);
                self.realtime_completion = completion;
                Ok(Value::Term(Ty::Text, value))
            }
            Node::NumberVector { items } => {
                let mut values = Vec::with_capacity(items.len());
                for item in items {
                    let value = self.node(item, env, path.clone())?;
                    need(
                        value.ty() == Ty::Integer,
                        "readonly Vector literal requires numbers",
                    )?;
                    values.push(value.scalar()?);
                }
                values::number_vector(values)
            }
            Node::Field { value, name } => {
                let acquired = self.node(value, env, path)?;
                if let Value::Term(Ty::Record(fields), expr) = &acquired {
                    if let Some(input) = math::domains::composite::input_name(expr) {
                        if let Some(domain) = self
                            .project
                            .entry_contexts
                            .get(self.selected_entry.id)
                            .and_then(|entry| entry.source_parameter_field_type(input, name))
                        {
                            // A checked scalar leaf of P has a public
                            // parameter path, as in the Java source route.
                            let sort = match (fields.get(name), domain) {
                                (Some(Ty::Integer), "number") => Some(ScalarSymbolType::Integer),
                                (Some(Ty::Bool), "bool") => Some(ScalarSymbolType::Boolean),
                                (Some(Ty::Text), "text") => Some(ScalarSymbolType::Text),
                                (Some(Ty::Text), "text?") => Some(ScalarSymbolType::NullableText),
                                (Some(record @ Ty::Record(_)), _) => {
                                    return Ok(Value::Term(
                                        record.clone(),
                                        math::domains::composite::input(&format!("{input}.{name}")),
                                    ))
                                }
                                _ => None,
                            };
                            if let Some(sort) = sort {
                                return Ok(Value::Term(
                                    fields[name].clone(),
                                    b::param(&format!("{input}.{name}"), sort),
                                ));
                            }
                        }
                    }
                }
                if name == "length" && matches!(&acquired, Value::Term(Ty::Vector(_), _)) {
                    let input = acquired.scalar()?;
                    let binding = math::domains::composite::input_name(&input)
                        .ok_or_else(|| fail("length requires a direct readonly Vector input"))?;
                    let id = self.selected_entry.id;
                    let entry = id.rsplit_once('.').map_or(id, |(_, method)| method);
                    need(
                        acquired.ty() == Ty::Vector(Box::new(Ty::Integer))
                            && self
                                .project
                                .parameter_value_types
                                .get(entry)
                                .and_then(|params| params.get(binding))
                                .is_some_and(|ty| ty == "vector<number>"),
                        "length requires a declared non-null readonly numeric Vector",
                    )?;
                    Ok(Value::Term(
                        Ty::Integer,
                        math::domains::composite::certified_vector_length(input),
                    ))
                } else {
                    values::field(acquired, name)
                }
            }
            Node::Settled { returns, steps } => {
                self.action_scope(returns, steps.as_slice(), env, path, true)
            }
            Node::Action { returns, steps } => self.action_scope(returns, steps, env, path, false),
            Node::Add { left, right } => {
                self.node_arithmetic(Arithmetic::Add, left, right, env, path)
            }
            Node::Sub { left, right } => {
                self.node_arithmetic(Arithmetic::Sub, left, right, env, path)
            }
            Node::Mul { left, right } => {
                self.node_arithmetic(Arithmetic::Mul, left, right, env, path)
            }
            Node::Var { name } => env
                .get(name)
                .cloned()
                .ok_or_else(|| fail(format!("unknown variable {name}"))),
            Node::Const(constant) => Ok(Value::Term(constant.ty.clone(), constant.value.clone())),
            Node::If {
                cond,
                then,
                otherwise,
            } => {
                let c = self.node(cond, env, path.clone())?;
                need(c.ty() == Ty::Bool, "if requires boolean")?;
                let c = self.condition_under_path(c.scalar()?, &path);
                if c.as_bool().is_some() {
                    self.branch_collapses += 1;
                    let branch = if c.as_bool() == Some(true) {
                        then
                    } else {
                        otherwise
                    };
                    let mut branch_env = env.clone();
                    return self.node(branch, &mut branch_env, path);
                }
                self.branch_splits += 1;
                let yes = self.node(then, &mut env.clone(), b::and(path.clone(), c.clone()))?;
                let no = self.node(otherwise, &mut env.clone(), b::and(path, b::not(c.clone())))?;
                self.choose(c, yes, no)
            }
            Node::Not { value } => {
                let v = self.node(value, env, path)?;
                need(v.ty() == Ty::Bool, "not type")?;
                Ok(Value::Term(Ty::Bool, b::not(v.scalar()?)))
            }
            Node::Eq { left, right } | Node::Gt { left, right } | Node::Lt { left, right } => {
                let l = self.node(left, env, path.clone())?;
                let r = self.node(right, env, path)?;
                need(l.ty() == r.ty(), "binary type mismatch")?;
                need(l.ty() != Ty::Text, "text content cannot substitute reference identity; define the required comparison")?;
                need(
                    matches!(l.ty(), Ty::Bool | Ty::Integer | Ty::Ref),
                    "composite equality is not declared",
                )?;
                let op = match n {
                    Node::Gt { .. } => BinOpKind::Gt,
                    Node::Lt { .. } => BinOpKind::Lt,
                    _ => BinOpKind::Eq,
                };
                need(
                    op == BinOpKind::Eq || l.ty() == Ty::Integer,
                    "ordered comparison requires numbers",
                )?;
                Ok(Value::Term(
                    Ty::Bool,
                    self.order_execution_comparison(op, l.scalar()?, r.scalar()?),
                ))
            }
        }
    }
    fn source_declared_type(&self, expression: &J, id: &str) -> Result<JavaType> {
        let owner = id
            .rsplit_once('.')
            .ok_or_else(|| fail("missing method owner"))?
            .0;
        match expression {
            // Source admission owns the instance-only receiver invariant.
            J::Ident(n) if n == "this" => return Ok(JavaType::Class(owner.into())),
            J::New { java_type, .. } => return Ok(java_type.clone()),
            J::ArrayAccess { target, .. } => {
                let JavaType::Array(item) = self.source_declared_type(target, id)? else {
                    return Err(fail("indexed field receiver requires a declared array"));
                };
                return Ok(*item);
            }
            J::MethodCall {
                target: Some(receiver),
                method,
                ..
            } => {
                if let Some(field) = crate::normalized::source_ports::receiver(receiver) {
                    if let Some(port) = self
                        .project
                        .source_ports
                        .get(&format!("{owner}.{field}.{method}"))
                    {
                        return Ok(port.returns.clone());
                    }
                }
            }
            J::FieldAccess { target, field } => {
                let value_type = self.source_declared_type(target, id)?;
                if let Some(product) =
                    crate::normalized::generic_product::instantiated(self.program, &value_type)
                {
                    return product
                        .field_type(field)
                        .ok_or_else(|| fail("undeclared generic product field"));
                }
                let JavaType::Class(owner) = value_type else {
                    return Err(fail("scalar has no fields"));
                };
                return self
                    .program
                    .find_class(&owner)
                    .and_then(|c| c.fields().iter().find(|f| f.name() == field))
                    .map(|f| f.java_type().clone())
                    .ok_or_else(|| fail("no such declared field"));
            }
            _ => {}
        }
        let J::Ident(name) = expression else {
            return Err(fail(
                "source field receiver must be a declared local or parameter",
            ));
        };
        let method = self.methods[id].syntax;
        if let Some((ty, _)) = method.params().iter().find(|(_, n)| n == name) {
            return Ok(ty.clone());
        }
        method
            .body()
            .unwrap()
            .iter()
            .find_map(|statement| match statement {
                S::VarDecl {
                    java_type,
                    name: local,
                    ..
                } if local == name => Some(java_type.clone()),
                _ => None,
            })
            .or_else(|| {
                if method.is_static() {
                    return None;
                }
                self.program
                    .find_class(owner)?
                    .fields()
                    .iter()
                    .find(|field| !field.is_static() && field.name() == name)
                    .map(|field| field.java_type().clone())
            })
            .ok_or_else(|| fail("source receiver declaration missing"))
    }
    fn source_reference_assignment(
        &self,
        expected: &JavaType,
        expression: &J,
        id: &str,
    ) -> Result<()> {
        let Some(owner) = self.project.source_owner(expected) else {
            return Ok(());
        };
        let interface = crate::normalized::specialized_interface(self.program, expected).is_some();
        if matches!(expected, JavaType::Generic(..)) {
            need(
                interface,
                "unsupported generic reference without specialized interface proof",
            )?;
        }
        // A null literal is assignment-compatible with a nominal reference,
        // but does not supply an owner or an object identity. Each arm of a
        // reference conditional must independently fit the declared target.
        match expression {
            J::Lit(JavaLit::Null) => return Ok(()),
            J::Ternary {
                then_expr,
                else_expr,
                ..
            } => {
                self.source_reference_assignment(expected, then_expr, id)?;
                return self.source_reference_assignment(expected, else_expr, id);
            }
            _ => {}
        }
        // Narrowing Object to a source class is never inferred from a heap
        // identity or a coincidental field name.
        let actual = self.source_declared_type(expression, id)?;
        need(
            (actual == *expected
                || (!interface && self.project.source_owner(&actual) == Some(owner)))
                || interface
                    && self
                        .project
                        .source_owner(&actual)
                        .and_then(|name| self.program.find_class(name))
                        .is_some_and(|class| {
                            crate::normalized::implements_specialization(
                                self.program,
                                class,
                                expected,
                            )
                        }),
            "source reference assignment requires the declared owner or interface type",
        )
    }
    fn source_field(&self, receiver: &J, field: &str, id: &str) -> Result<()> {
        let ty = self.source_declared_type(receiver, id)?;
        if let Some(product) = crate::normalized::generic_product::instantiated(self.program, &ty) {
            return need(
                product.field_type(field).is_some(),
                "undeclared generic product field",
            );
        }
        let owner = self
            .project
            .source_owner(&ty)
            .ok_or_else(|| fail("source field receiver has no declared owner layout"))?;
        need(
            self.project.source_layouts[owner].contains_key(field),
            "field is absent from declared receiver owner",
        )
    }
    fn block(&mut self, body: &[S], id: &str, env: &mut Env, path: Expr) -> Result<Value> {
        self.source_steps(body, id, env, path.clone())?;
        let terminal = self.realtime_completion.terminal_alternatives();
        if let Some(result) = terminal
            .as_ref()
            .and_then(|exits| exits.normal_return_value())
        {
            return Ok(Value::Term(
                self.methods[id].returns.clone(),
                self.normalize_execution_value(result),
            ));
        }
        if self.methods[id].syntax.is_constructor() {
            return Ok(Value::Term(Ty::Unit, b::lit_null()));
        }
        if self.methods[id].returns == Ty::Unit {
            self.realtime_completion = self
                .realtime_completion
                .replace_continue(realtime_state::Completion::Return(b::lit_null()));
            return Ok(Value::Term(Ty::Unit, b::lit_null()));
        }
        if terminal
            .as_ref()
            .is_some_and(|exits| exits.is_always_throwing())
            || math::normalize(self.normal_path(path)).as_bool() == Some(false)
        {
            // For a nested call only its actual invocation domain must
            // complete. All of that domain can throw while the otherwise
            // unreachable complement still appears as Continue.
            // No expression can observe this carrier; the completion records
            // the actual exit and enclosing catches consume that completion.
            return Ok(Value::Term(self.methods[id].returns.clone(), b::lit_null()));
        }
        Err(fail("missing return"))
    }

    // The adjacent map_empty instruction supplies the map domain. A closed
    // source class is just its nominal carrier; it gives no ordinary Java
    // operations or aliases access to the map value.
    fn declared_local_text_map(&self, declaration: &JavaType) -> bool {
        matches!(declaration, JavaType::Class(name)
            if self.program.find_class(name).is_some_and(|owner| !owner.is_interface()))
            || super::source_map::declared_map(declaration)
    }

    pub(super) fn source_steps(
        &mut self,
        body: &[S],
        id: &str,
        env: &mut Env,
        entry_path: Expr,
    ) -> Result<()> {
        self.source_steps_from(body, 0, id, env, entry_path)
    }

    fn source_steps_from(
        &mut self,
        body: &[S],
        start: usize,
        id: &str,
        env: &mut Env,
        entry_path: Expr,
    ) -> Result<()> {
        for (i, s) in body.iter().enumerate().skip(start) {
            let continuing = self.realtime_completion.continuing_guard();
            let path = math::normalize(b::and(entry_path.clone(), continuing.clone()));
            if path.as_bool() == Some(false) {
                break;
            }
            if self.realtime_completion.has_pending_switch_break() {
                // Run the suffix once on its live domain, preserving the
                // locals at the already-broken paths for switch exit.
                return self.source_branch(
                    continuing,
                    env,
                    entry_path,
                    |engine, locals, active| engine.source_steps_from(body, i, id, locals, active),
                    |_, _, _| Ok(()),
                );
            }
            self.budget()?;
            match s {
                S::Block(statements) => self.source_scope(statements, id, env, path)?,
                // The built-in Object constructor has no source-visible
                // operation; this marker delimits the checked prologue.
                S::ObjectSuperConstructor => {}
                S::ThisConstructor(args) => {
                    let owner = id.rsplit_once('.').expect("constructor key has an owner").0;
                    let class = self
                        .program
                        .find_class(owner)
                        .ok_or_else(|| fail("no constructor owner"))?;
                    let (target, _) = crate::normalized::constructor_for_arity(class, args.len())
                        .ok_or_else(|| fail("unresolved same-class constructor"))?;
                    let receiver = env.get("this").cloned().ok_or_else(|| {
                        fail("constructor delegation needs the original receiver")
                    })?;
                    let Some((values, path)) = self.source_arguments(args, id, env, path)? else {
                        continue;
                    };
                    self.java_call_on(&target, values, path, Some(receiver))?;
                }
                S::Return(Some(expr)) => {
                    self.source_reference_assignment(
                        self.methods[id].syntax.return_type(),
                        expr,
                        id,
                    )?;
                    let value = self.java_expr(expr, id, env, path.clone())?;
                    if self.normal_path(path).as_bool() == Some(false) {
                        return Ok(());
                    }
                    let value = if crate::normalized::native_decimal_type(
                        self.methods[id].syntax.return_type(),
                    ) {
                        Self::promote_native_decimal(value)
                    } else {
                        value
                    };
                    let realtime_value = self.normalize_execution_value(value.scalar()?);
                    self.realtime_completion = self
                        .realtime_completion
                        .replace_continue(realtime_state::Completion::Return(realtime_value));
                    return Ok(());
                }
                S::Return(None) => {
                    self.realtime_completion = self
                        .realtime_completion
                        .replace_continue(realtime_state::Completion::Return(b::lit_null()));
                    return Ok(());
                }
                S::Throw(J::Lit(JavaLit::Null)) => {
                    // The Java throw itself raises NullPointerException;
                    // a null value is not an exception payload in this case.
                    self.throw_source_on_path(path, "java.lang.NullPointerException");
                }
                S::Throw(expr) => {
                    use crate::normalized::NativeThrowMessage;
                    let (class, payload) = match crate::normalized::native_literal_throw(expr) {
                        Some((class, message)) => (
                            class.into(),
                            match message {
                                NativeThrowMessage::Absent => None,
                                NativeThrowMessage::Null => Some(b::lit_null()),
                                NativeThrowMessage::Text(text) => Some(b::lit_str(text)),
                                NativeThrowMessage::TextUnits(units) => {
                                    Some(b::lit(math::Lit::from_text_units(units.to_vec())))
                                }
                            },
                        ),
                        None => {
                            let class = crate::normalized::type_only_throw_name(s)
                                .and_then(|raw| {
                                    self.project.source_exception_bindings.get(id)?.get(raw)
                                })
                                .ok_or_else(|| {
                                    fail("source throw requires an admitted exception constructor")
                                })?
                                .clone();
                            (class, None)
                        }
                    };
                    self.realtime_throws.push(path);
                    self.realtime_completion = self
                        .realtime_completion
                        .replace_continue(realtime_state::Completion::Throw { class, payload });
                }
                S::VarDecl {
                    java_type,
                    name,
                    initializer: None,
                    ..
                } => {
                    let initializes = body
                        .get(i + 1)
                        .and_then(|stmt| LocalIrInitializer::for_binding(stmt, name))
                        .or_else(|| {
                            // Exact values may declare several consecutive locals and
                            // then bind them in a consecutive directive block. Do not
                            // look past ordinary execution or a different IR family.
                            if !crate::normalized::exact_decimal_type(java_type) {
                                return None;
                            }
                            body[i + 1..]
                                .iter()
                                .skip_while(|stmt| {
                                    matches!(stmt, S::VarDecl { java_type, initializer: None, .. }
                                    if crate::normalized::exact_decimal_type(java_type))
                                })
                                .take_while(|stmt| {
                                    matches!(
                                        stmt,
                                        S::LocalIr(normalizer_ir::local::LocalIr::Decimal(_))
                                    )
                                })
                                .find_map(|stmt| LocalIrInitializer::for_binding(stmt, name))
                        });
                    let binds = match initializes {
                        Some(LocalIrInitializer::TextMap) => {
                            self.declared_local_text_map(java_type)
                        }
                        Some(LocalIrInitializer::NullableText) => {
                            super::source_map::declared_text(java_type)
                        }
                        Some(LocalIrInitializer::MemberChoice) => {
                            self.project.parameter_type(java_type)? == Ty::Integer
                                || self.project.source_owner(java_type).is_some()
                        }
                        Some(LocalIrInitializer::Integer) => {
                            self.project.parameter_type(java_type)? == Ty::Integer
                        }
                        Some(LocalIrInitializer::Relation) => true,
                        Some(LocalIrInitializer::Decimal) => {
                            crate::normalized::exact_decimal_type(java_type)
                        }
                        None => false,
                    };
                    need(
                        binds && !env.contains_key(name),
                        "uninitialized local requires a following typed local IR definition",
                    )?;
                }
                S::LocalIr(normalizer_ir::local::LocalIr::LocalMapEmpty { binding }) => {
                    need(
                        preceding_uninitialized(body, i, binding)
                            .is_some_and(|ty| self.declared_local_text_map(ty)),
                        "local map requires a fresh typed local declaration",
                    )?;
                    env.insert(binding.clone(), Value::LocalTextMap(Default::default()));
                }
                S::LocalIr(normalizer_ir::local::LocalIr::LocalMapPut {
                    binding,
                    key,
                    value,
                }) => {
                    let value = super::source_map::text_operand(value, env)?;
                    let Some(Value::LocalTextMap(map)) = env.get_mut(binding) else {
                        return Err(fail("local map update requires its fresh map binding"));
                    };
                    map.put(key.clone(), value);
                }
                S::LocalIr(normalizer_ir::local::LocalIr::LocalMapGet {
                    binding,
                    source,
                    key,
                    fallback,
                }) => {
                    need(
                        preceding_uninitialized(body, i, binding)
                            .is_some_and(super::source_map::declared_text),
                        "local map read requires an uninitialized text local",
                    )?;
                    let method = self.methods[id].syntax;
                    let owner = id
                        .rsplit_once('.')
                        .and_then(|(owner, _)| self.program.find_class(owner));
                    need(
                        owner.is_some_and(|owner| {
                            crate::normalized::nullable_text_return(owner, method)
                        }),
                        "local map text result must declare a nullable return",
                    )?;
                    let fallback = super::source_map::text_operand(fallback, env)?;
                    let Some(Value::LocalTextMap(map)) = env.get(source) else {
                        return Err(fail("local map read requires its fresh map binding"));
                    };
                    env.insert(
                        binding.clone(),
                        Value::Term(Ty::Text, map.get_or(key, fallback)),
                    );
                }
                S::LocalIr(normalizer_ir::local::LocalIr::ProjectedUnionCount {
                    binding,
                    first,
                    remaining,
                }) => {
                    need(
                        preceding_uninitialized(body, i, binding).is_some(),
                        "projected union count must initialize its declared local",
                    )?;
                    self.projected_union_count(binding, first, remaining, env)?;
                }
                S::LocalIr(normalizer_ir::local::LocalIr::Relation(query)) => {
                    let decl = self.source_declared_type(&J::Ident(query.binding.clone()), id)?;
                    self.execute_query(query, Some(&decl), env, &path)?;
                }
                S::LocalIr(normalizer_ir::local::LocalIr::Decimal(decimal)) => {
                    let decl = self.source_declared_type(&J::Ident(decimal.binding.clone()), id)?;
                    self.execute_source_decimal(decimal, &decl, env)?;
                }
                S::LocalIr(normalizer_ir::local::LocalIr::SqlWrite(write)) => {
                    let declared = preceding_uninitialized(body, i, &write.binding).is_some();
                    need(
                        declared,
                        "local IR result must bind a declared number local",
                    )?;
                    self.execute_sql_write(write, env, path.clone())?;
                }
                S::VarDecl {
                    java_type,
                    name,
                    initializer: Some(expr),
                    ..
                } => {
                    // Source admission has already checked the Java lexical
                    // declaration space, including parameters and enclosing locals.
                    if crate::normalized::host_collection_allocation(java_type, expr) {
                        crate::preprocess::out_of_source(
                            self.selected_entry.id,
                            "java.util.ArrayList.<init>",
                        );
                    }
                    self.source_reference_assignment(java_type, expr, id)?;
                    let v = self.java_expr(expr, id, env, path.clone())?;
                    if self.normal_path(path).as_bool() == Some(false) {
                        continue;
                    }
                    let v = if crate::normalized::native_decimal_type(java_type) {
                        Self::promote_native_decimal(v)
                    } else {
                        v
                    };
                    v.check_java_array_component(primitive_array_component(java_type))?;
                    need(
                        v.ty() != Ty::LocalRelation,
                        "local relation requires a typed query, not a Java scalar binding",
                    )?;
                    need(
                        if crate::normalized::generic_product::instantiated(self.program, java_type)
                            .is_some()
                        {
                            v.ty() == Ty::Ref
                        } else {
                            v.ty() == self.project.declared_result_type(java_type)?
                        } || (matches!(v.ty(), Ty::Record(_))
                            && self.project.source_owner(java_type).is_some()
                            && self.source_declared_type(expr, id)? == *java_type),
                        "local type mismatch",
                    )?;
                    env.insert(name.clone(), v);
                }
                S::Assign {
                    target: J::ArrayAccess { target, index },
                    value,
                    ..
                } => {
                    let array = self.java_expr(target, id, env, path.clone())?;
                    let path = self.normal_path(path);
                    if path.as_bool() == Some(false) {
                        continue;
                    }
                    let index = self.java_expr(index, id, env, path.clone())?;
                    let path = self.normal_path(path);
                    if path.as_bool() == Some(false) {
                        continue;
                    }
                    // A simple store evaluates its RHS before null and
                    // bounds checks, but not after failed operand evaluation.
                    let rhs = self.java_expr(value, id, env, path.clone())?;
                    let path = self.normal_path(path);
                    if path.as_bool() == Some(false) {
                        continue;
                    }
                    self.source_array_store(array, index, rhs, path)?;
                }
                S::Assign {
                    target: J::Ident(name),
                    value,
                    ..
                } => {
                    if let Some(local) = env.get(name) {
                        let ty = local.ty();
                        need(ty != Ty::LocalTextMap,
                            "local map carrier can only be used through declared map operators; Java assignment cannot rebind it")?;
                        let java_component = local.java_array_component().map(str::to_owned);
                        let value = self.java_expr(value, id, env, path.clone())?;
                        if self.normal_path(path).as_bool() == Some(false) {
                            continue;
                        }
                        let value = if ty == Ty::Decimal {
                            Self::promote_native_decimal(value)
                        } else {
                            value
                        };
                        value.check_java_array_component(java_component.as_deref())?;
                        need(value.ty() == ty, "local assignment type mismatch")?;
                        env.insert(name.clone(), value);
                    } else {
                        let owner = id.rsplit_once('.').unwrap().0;
                        if let Some(receiver) = env.get("this").cloned().filter(|_| {
                            self.program.find_class(owner).is_some_and(|class| {
                                class
                                    .fields()
                                    .iter()
                                    .any(|field| !field.is_static() && field.name() == name)
                            })
                        }) {
                            let Some((value, path)) = self.source_operand(value, id, env, path)?
                            else {
                                continue;
                            };
                            self.write_source_field(&receiver.scalar()?, name, value, path)?;
                        } else if !self.methods[id].syntax.is_static()
                            && !env.contains_key("this")
                            && self.is_entry_state(owner, name)
                        {
                            let Some((value, path)) = self.source_operand(value, id, env, path)?
                            else {
                                continue;
                            };
                            self.write_entry_state(owner, name, value, path)?;
                        } else {
                            return Err(fail(format!("unbound Java assignment target {name}")));
                        }
                    }
                }
                S::Assign {
                    target: J::FieldAccess { target, field },
                    value,
                    ..
                } => {
                    self.source_field(target, field, id)?;
                    if matches!(target.as_ref(), J::Ident(name) if name == "this")
                        && !env.contains_key("this")
                    {
                        let Some((value, path)) = self.source_operand(value, id, env, path)? else {
                            continue;
                        };
                        self.write_entry_state(id.rsplit_once('.').unwrap().0, field, value, path)?;
                        continue;
                    }
                    let reference = self.java_expr(target, id, env, path.clone())?;
                    let path = self.normal_path(path);
                    if path.as_bool() == Some(false) {
                        continue;
                    }
                    if matches!(reference.ty(), Ty::Record(_)) {
                        return Err(crate::preprocess::required(
                            "NORM-SOURCE-TYPE",
                            id,
                            &format!(
                                "field {field} of the acquired immutable record is not writable"
                            ),
                            "保留原始字段写入目标；不可把只读记录投影当作可写堆对象。",
                        ));
                    }
                    // Java evaluates RHS before the target putfield null check,
                    // but only if evaluating the receiver itself completed.
                    let value = self.java_expr(value, id, env, path.clone())?;
                    let path = self.normal_path(path);
                    if path.as_bool() == Some(false) {
                        continue;
                    }
                    self.write_source_field(&reference.scalar()?, field, value, path)?;
                }
                S::If {
                    cond,
                    then_block,
                    else_block,
                } => {
                    let (test, matched) = if let Some(pattern) = cond.direct_type_pattern() {
                        need(
                            crate::normalized::platform_string_type(self.program, pattern.target),
                            "uncertified Java pattern target",
                        )?;
                        let Some((value, _)) =
                            self.source_operand(pattern.operand, id, env, path.clone())?
                        else {
                            continue;
                        };
                        let (guard, narrowed) = self.java_string_pattern(value)?;
                        let guard = if pattern.matched_when_true {
                            guard
                        } else {
                            b::not(guard)
                        };
                        let survives = S::pattern_in_continuation(
                            pattern.matched_when_true,
                            then_block,
                            else_block.as_deref(),
                        );
                        (
                            Value::Term(Ty::Bool, guard),
                            Some(super::source_control::SourcePattern {
                                name: pattern.name,
                                value: narrowed,
                                matched_when_true: pattern.matched_when_true,
                                survives,
                            }),
                        )
                    } else {
                        (self.java_expr(cond, id, env, path.clone())?, None)
                    };
                    let path = self.normal_path(path);
                    if path.as_bool() == Some(false) {
                        continue;
                    }
                    let condition = self.condition_under_path(test.scalar()?, &path);
                    self.source_if(
                        condition,
                        then_block,
                        else_block.as_deref().unwrap_or_default(),
                        matched,
                        id,
                        env,
                        path,
                    )?;
                }
                S::SwitchBreak => {
                    self.realtime_completion = self
                        .realtime_completion
                        .replace_continue(realtime_state::Completion::SwitchBreak);
                    return Ok(());
                }
                S::Switch {
                    selector,
                    cases,
                    default,
                    fall_through,
                } => {
                    let selector = self.java_expr(selector, id, env, path.clone())?;
                    let path = self.normal_path(path);
                    if path.as_bool() == Some(false) {
                        continue;
                    }
                    let ty = selector.ty();
                    need(
                        matches!(ty, Ty::Integer | Ty::Text),
                        "switch requires an exact integer or text selector",
                    )?;
                    let selector = selector.scalar()?;
                    let nonnull_text = matches!(selector.as_sym_var(),
                        Some(SymVarOrigin::Parameter { primitive_type, .. }
                            | SymVarOrigin::External { primitive_type, .. }
                            | SymVarOrigin::Temporal { primitive_type, .. }) if primitive_type == "text");
                    if ty == Ty::Text && !nonnull_text {
                        self.throw_source_on_path(
                            b::and(path.clone(), b::eq(selector.clone(), b::lit_null())),
                            "java.lang.NullPointerException",
                        );
                    }
                    let path = self.normal_path(path);
                    if path.as_bool() != Some(false) {
                        // A nested switch cannot consume a break belonging
                        // to its parent, even when symbolic arms coexist.
                        use realtime_state::Completion;
                        let parent =
                            std::mem::replace(&mut self.realtime_completion, Completion::Continue);
                        if let Some(flow) = fall_through {
                            self.source_switch_through(
                                selector, cases, default, flow, id, env, path,
                            )?;
                        } else {
                            self.source_switch(selector, cases, default, id, env, path)?;
                        }
                        let local =
                            std::mem::replace(&mut self.realtime_completion, Completion::Continue);
                        self.realtime_completion = parent.replace_continue(local.finish_switch());
                    }
                }
                S::For {
                    init,
                    cond,
                    update,
                    body,
                } => {
                    if let Some(proof) =
                        crate::normalized::fixed_for::BoundedControlFor::from_for(s)
                    {
                        self.execute_bounded_control_for(proof, id, env, path)?;
                        continue;
                    }
                    if let Some(field) = crate::normalized::fixed_for::FixedFieldFor::from_for(s) {
                        self.execute_fixed_field_for(field, body, id, env, path)?;
                        continue;
                    }
                    if let Some(rectangle) =
                        crate::normalized::fixed_for::RectangularCounter::from_for(s)
                    {
                        self.execute_rectangular_counter(rectangle, env)?;
                        continue;
                    }
                    let header = crate::normalized::fixed_for::FixedFor::from_header(
                        init.as_deref(),
                        cond.as_ref(),
                        update.as_ref(),
                    )
                    .ok_or_else(|| fail("for loop needs a proven fixed induction bound"))?;
                    if let Some(declarations) = header.declarations {
                        let mut locals = env.clone();
                        self.source_steps(declarations, id, &mut locals, path.clone())?;
                        locals.remove(header.counter);
                        self.execute_fixed_for(header, body, id, &mut locals, path)?;
                        for (name, value) in env.iter_mut() {
                            *value = locals
                                .remove(name)
                                .expect("loop scope preserved its outer binding");
                        }
                    } else {
                        self.execute_fixed_for(header, body, id, env, path)?;
                    }
                }
                S::Try {
                    resource,
                    body,
                    catches,
                    finally_block,
                } => {
                    self.execute_source_try(
                        resource.as_deref(),
                        body,
                        catches,
                        finally_block.as_deref(),
                        id,
                        env,
                        path,
                    )?;
                }
                S::LocalIr(normalizer_ir::local::LocalIr::MemberChoice {
                    binding,
                    members,
                    empty,
                }) => {
                    let java_type = preceding_uninitialized(body, i, binding).ok_or_else(|| {
                        fail("member choice must follow its uninitialized local declaration")
                    })?;
                    self.choose_source_member(binding, members, empty, java_type, id, env, path)?;
                }
                S::LocalIr(normalizer_ir::local::LocalIr::IntegerRange(range)) => {
                    let declared = preceding_uninitialized(body, i, &range.binding)
                        .is_some_and(crate::normalized::integral);
                    self.source_integer_range(range, env, &path, declared)?;
                }
                S::ExprStmt(call @ J::MethodCall { .. }) => {
                    self.java_expr(call, id, env, path)?;
                }
                S::ForEach {
                    var_name,
                    iterable,
                    body,
                    ..
                } => {
                    let (total, offset) = crate::normalized::array_sum(var_name, body)
                        .ok_or_else(|| fail("for-each requires an exact source array sum"))?;
                    let array = self.java_expr(iterable, id, env, path.clone())?;
                    let path = self.normal_path(path);
                    if path.as_bool() == Some(false) {
                        continue;
                    }
                    let prior = env
                        .get(total)
                        .ok_or_else(|| fail("array sum needs an initialized accumulator"))?;
                    need(
                        prior.ty() == Ty::Integer,
                        "array sum needs a numeric accumulator",
                    )?;
                    let sum = self.source_array_sum(array, prior.scalar()?, offset, path)?;
                    env.insert(total.into(), sum);
                }
                _ => return Err(fail("unsupported Java statement in pure-array-1")),
            }
        }
        Ok(())
    }
    fn promote_native_decimal(value: Value) -> Value {
        match value {
            Value::Term(Ty::Integer, expr) | Value::IntegerFloor { value: expr, .. } => {
                Value::Term(Ty::Decimal, expr)
            }
            value => value,
        }
    }

    fn java_arithmetic(
        &mut self,
        op: Arithmetic,
        operands: (&J, &J),
        id: &str,
        env: &mut Env,
        path: Expr,
    ) -> Result<Value> {
        let Some((left, right, path)) = self.source_binary_operands(operands, id, env, path)?
        else {
            // All operand paths completed exceptionally. Enclosing evaluation
            // must consult that completion before inspecting this carrier.
            return Ok(Value::Term(Ty::Integer, b::lit_null()));
        };
        if op == Arithmetic::Add && (left.ty() == Ty::Text || right.ty() == Ty::Text) {
            let (left, right) = (left.scalar()?, right.scalar()?);
            let text = |value: Expr| match value.as_lit() {
                Some(literal) if literal.is_text() => Some(value),
                Some(math::Lit::Null) => Some(b::lit_str("null")),
                _ => None,
            };
            if let (Some(left), Some(right)) = (text(left), text(right)) {
                return Ok(Value::Term(Ty::Text, math::normalize(b::add(left, right))));
            }
            return Err(fail(
                "text concatenation requires concrete text or null operands",
            ));
        }
        if left.ty() == Ty::Decimal || right.ty() == Ty::Decimal {
            need(
                matches!(left.ty(), Ty::Integer | Ty::Decimal)
                    && matches!(right.ty(), Ty::Integer | Ty::Decimal),
                "numeric operands required",
            )?;
            use math::domains::decimal::Operator;
            let binary = match op {
                Arithmetic::Add => Operator::Add,
                Arithmetic::Sub => Operator::Sub,
                Arithmetic::Mul => Operator::Mul,
            };
            return Ok(Value::Term(
                Ty::Decimal,
                math::domains::decimal::binary(binary, left.scalar()?, right.scalar()?),
            ));
        }
        need(
            left.ty() == Ty::Integer && right.ty() == Ty::Integer,
            "integer operands required",
        )?;
        if op == Arithmetic::Add
            && (matches!(left, Value::IntegerFloor { .. })
                || matches!(right, Value::IntegerFloor { .. }))
        {
            let domain = self.normal_path(path.clone());
            if let (Some(left_floor), Some(right_floor)) = (
                left.certified_integer_floor(&domain),
                right.certified_integer_floor(&domain),
            ) {
                let value = self.normalize_execution_value(b::add(left.scalar()?, right.scalar()?));
                return Ok(Value::IntegerFloor {
                    value,
                    lower: left_floor + right_floor,
                    domain,
                });
            }
        }
        // The same acquired count is a total exact integer on this path.
        // Distinct SQL acquisition identities remain part of expression equality.
        if op == Arithmetic::Sub
            && matches!(left, Value::IntegerFloor { .. })
            && matches!(right, Value::IntegerFloor { .. })
            && left.scalar()? == right.scalar()?
        {
            let path = self.normal_path(path.clone());
            if left.certified_integer_floor(&path).is_some()
                && right.certified_integer_floor(&path).is_some()
            {
                return Ok(Value::Term(Ty::Integer, b::lit_int(0)));
            }
        }
        let (left, right) = (left.scalar()?, right.scalar()?);
        // Operand evaluation is already recorded in S. An identical
        // numeric receipt can cancel only on a path that acquired it.
        if op == Arithmetic::Sub && left == right && self.acquired_integer_receipt(&left, &path) {
            return Ok(Value::Term(Ty::Integer, b::lit_int(0)));
        }
        if op == Arithmetic::Mul {
            let zero = b::lit_int(0);
            if (left == zero && self.acquired_integer_receipt(&right, &path))
                || (right == zero && self.acquired_integer_receipt(&left, &path))
            {
                // The product no longer needs the value. Its already
                // recorded call and unsuccessful completions still matter.
                return Ok(Value::Term(Ty::Integer, zero));
            }
        }
        Ok(Value::Term(
            Ty::Integer,
            self.normalize_execution_value(op.build(left, right)),
        ))
    }

    fn evaluate_local_array_initializer(
        &mut self,
        element_type: &JavaType,
        elements: &[J],
        id: &str,
        env: &mut Env,
        path: Expr,
    ) -> Result<super::source_arrays::LocalIntArray> {
        need(
            crate::normalized::java_int(element_type),
            "unsupported Java expression in pure-array-1",
        )?;
        let Some((values, _)) = self.source_arguments(elements, id, env, path)? else {
            return Ok(super::source_arrays::LocalIntArray::Unreachable);
        };
        self.source_local_array_initializer(values)
    }

    fn evaluate_local_array_allocation(
        &mut self,
        element_type: &JavaType,
        length: &J,
        id: &str,
        env: &mut Env,
        path: Expr,
    ) -> Result<super::source_arrays::LocalIntArray> {
        need(
            crate::normalized::java_int(element_type),
            "unsupported Java expression in pure-array-1",
        )?;
        let size = self.java_expr(length, id, env, path.clone())?;
        let path = self.normal_path(path);
        if path.as_bool() == Some(false) {
            return Ok(super::source_arrays::LocalIntArray::Unreachable);
        }
        self.source_local_array_allocation(size, path)
    }

    fn source_java_array_length(
        &mut self,
        target: &J,
        id: &str,
        env: &mut Env,
        path: Expr,
    ) -> Result<Option<Expr>> {
        // An explicitly declared entry-state array is a non-null E root as
        // well as an ordinary Java array. Do not probe scalar state fields
        // or a local object that merely has a field named length.
        let state_name = match target {
            J::Ident(name) if !env.contains_key(name) => Some(name.as_str()),
            J::FieldAccess { target, field } if matches!(target.as_ref(), J::Ident(name) if name == "this") => {
                Some(field.as_str())
            }
            _ => None,
        };
        if let Some(name) = state_name {
            let owner = id.rsplit_once('.').unwrap().0;
            if !self.methods[id].syntax.is_static()
                && !env.contains_key("this")
                && self.program.find_class(owner).is_some_and(|class| {
                    class.fields().iter().any(|field| {
                        field.name() == name
                            && crate::normalized::entry_state(class, field)
                            && crate::normalized::numeric_array(field.java_type())
                    })
                })
            {
                self.budget()?;
                self.read_entry_state(owner, name, &path)?;
                return Ok(Some(super::source_arrays::borrowed_array_length(
                    &source_state_symbol(name),
                )));
            }
        }
        match target {
            J::Ident(name) => {
                if let Some(Value::LocalIntArray(array)) = env.get(name) {
                    // Charge the child identifier visit just as generic expression
                    // evaluation did; the immutable length itself needs no path.
                    self.budget()?;
                    return Ok(Some(self.normalize_execution_value(array.length_expr())));
                }
                if let Some(Value::Sequence {
                    item,
                    name: root,
                    null,
                    ..
                }) = env.get(name)
                {
                    if *item != Ty::Integer
                        && !self.source_relations.record_inputs.contains_key(root)
                    {
                        return Ok(None);
                    }
                    // This Java expression path has no adapter-signature
                    // body. Length does not read or convert any element.
                    // Keep the actual P or E root of a local alias.
                    let state_root = self
                        .source_objects
                        .state_bindings
                        .values()
                        .any(|name| source_state_symbol(name) == *root);
                    let admitted = state_root
                        || self
                            .selected_entry
                            .syntax
                            .params()
                            .iter()
                            .any(|(_, parameter)| parameter == root);
                    if admitted {
                        self.budget()?;
                        if self.source_nonnull_array_path(path, null.clone()).as_bool()
                            == Some(false)
                        {
                            return Ok(Some(b::lit_null()));
                        }
                        return Ok(Some(super::source_arrays::borrowed_array_length(root)));
                    }
                }
                Ok(None)
            }
            J::ArrayInitializer {
                element_type,
                elements,
            } => {
                self.budget()?;
                need(
                    crate::normalized::int_array_component(element_type),
                    "unsupported Java array component",
                )?;
                // Evaluate each initializer before observing the outer
                // length. Nested lengths are not added together.
                if crate::normalized::java_int(element_type) {
                    let Some((values, _)) = self.source_arguments(elements, id, env, path)? else {
                        return Ok(Some(b::lit_null()));
                    };
                    need(
                        values.iter().all(|value| value.ty() == Ty::Integer),
                        "expected Java int for local array",
                    )?;
                } else {
                    let mut live = self.normal_path(path);
                    for element in elements {
                        if live.as_bool() == Some(false) {
                            return Ok(Some(b::lit_null()));
                        }
                        if let J::ArrayInitializer { .. } = element {
                            self.source_java_array_length(element, id, env, live.clone())?;
                            live = self.normal_path(live);
                        } else {
                            // A reference element is evaluated, but not
                            // dereferenced; a null row is still one slot.
                            let Some((_, next)) = self.source_operand(element, id, env, live)?
                            else {
                                return Ok(Some(b::lit_null()));
                            };
                            live = next;
                        }
                    }
                    if live.as_bool() == Some(false) {
                        return Ok(Some(b::lit_null()));
                    }
                }
                Ok(Some(b::lit_int(elements.len() as i64)))
            }
            J::ArrayAllocation {
                element_type,
                length,
            } => {
                self.budget()?;
                let path = self.normal_path(path);
                let length = self
                    .evaluate_local_array_allocation(element_type, length, id, env, path)?
                    .length();
                Ok(Some(b::lit_int(length)))
            }
            _ => Ok(None),
        }
    }

    fn source_arguments(
        &mut self,
        args: &[J],
        id: &str,
        env: &mut Env,
        mut path: Expr,
    ) -> Result<Option<(Vec<Value>, Expr)>> {
        path = self.normal_path(path);
        if path.as_bool() == Some(false) {
            return Ok(None);
        }
        let mut values = Vec::with_capacity(args.len());
        for arg in args {
            let Some((value, live)) = self.source_operand(arg, id, env, path)? else {
                return Ok(None);
            };
            values.push(value);
            path = live;
        }
        Ok(Some((values, path)))
    }

    fn source_operand(
        &mut self,
        operand: &J,
        id: &str,
        env: &mut Env,
        path: Expr,
    ) -> Result<Option<(Value, Expr)>> {
        let path = self.normal_path(path);
        if path.as_bool() == Some(false) {
            return Ok(None);
        }
        let value = self.java_expr(operand, id, env, path.clone())?;
        let path = self.normal_path(path);
        Ok((path.as_bool() != Some(false)).then_some((value, path)))
    }

    fn source_binary_operands(
        &mut self,
        (left, right): (&J, &J),
        id: &str,
        env: &mut Env,
        path: Expr,
    ) -> Result<Option<(Value, Value, Expr)>> {
        let Some((left, path)) = self.source_operand(left, id, env, path)? else {
            return Ok(None);
        };
        let Some((right, path)) = self.source_operand(right, id, env, path)? else {
            return Ok(None);
        };
        Ok(Some((left, right, path)))
    }

    fn java_string_pattern(&self, value: Value) -> Result<(Expr, Value)> {
        let reference = value.scalar()?;
        let (guard, text) = if reference.is_null() {
            // No projection is consumed on an unmatched arm.
            (b::lit_bool(false), reference)
        } else if value.ty() == Ty::Text {
            (b::ne(reference.clone(), b::lit_null()), reference)
        } else if matches!(reference.as_sym_var(), Some(math::SymVarOrigin::Parameter { primitive_type, .. })
            if primitive_type == "java_reference")
        {
            use math::domains::source_type_guard::{certified_text, is_native_text};
            (is_native_text(reference.clone()), certified_text(reference))
        } else if self.source_objects.allocation_owner(&reference).is_some() {
            (b::lit_bool(false), b::lit_null())
        } else {
            return Err(fail("String instanceof needs a supported source reference"));
        };
        Ok((guard, Value::Term(Ty::Text, text)))
    }

    pub(super) fn java_expr(
        &mut self,
        j: &J,
        id: &str,
        env: &mut Env,
        path: Expr,
    ) -> Result<Value> {
        self.budget()?;
        let path = self.normal_path(path);
        match j {
            J::Ident(n) => {
                if let Some(value) = env.get(n) {
                    return Ok(value.clone());
                }
                let owner = id.rsplit_once('.').unwrap().0;
                let entry = self.selected_entry;
                if n == "this"
                    && !self.methods[id].syntax.is_static()
                    && !entry.syntax.is_static()
                    && entry
                        .id
                        .rsplit_once('.')
                        .is_some_and(|(selected, _)| selected == owner)
                {
                    return Ok(self.source_entry_receiver_value(owner));
                }
                if !self.methods[id].syntax.is_static() && !env.contains_key("this") {
                    if self.active_static_receiver.is_none() {
                        if let Some(value) = self.assembled_dependency(owner, n) {
                            return Ok(value);
                        }
                    }
                    if self.is_entry_state(owner, n) {
                        return self.read_entry_state(owner, n, &path);
                    }
                }
                if let Some(receiver) = env.get("this") {
                    if self.program.find_class(owner).is_some_and(|class| {
                        class
                            .fields()
                            .iter()
                            .any(|field| !field.is_static() && field.name() == n)
                    }) {
                        return self.read_or_bind_source_field(&receiver.scalar()?, n, &path);
                    }
                }
                Err(fail(format!("undefined Java variable {n}")))
            }
            J::InstanceOf {
                value,
                target,
                binding,
            } => {
                need(binding.is_none(), "uncertified Java pattern binding")?;
                let Some((value, _)) = self.source_operand(value, id, env, path)? else {
                    return Ok(Value::Term(Ty::Bool, b::lit_null()));
                };
                if crate::normalized::platform_string_type(self.program, target) {
                    return Ok(Value::Term(Ty::Bool, self.java_string_pattern(value)?.0));
                }
                let value = value.scalar()?;
                if crate::normalized::root_object_type(self.program, target)
                    && matches!(value.as_sym_var(), Some(math::SymVarOrigin::Parameter { primitive_type, .. })
                        if primitive_type == "java_reference")
                {
                    let present = self.source_reference_equality(value, b::lit_null(), true)?;
                    return Ok(Value::Term(Ty::Bool, present));
                }
                let JavaType::Class(target) = target else {
                    return Err(fail("instanceof target is not nominal"));
                };
                let present = if value.is_null() {
                    false
                } else {
                    let actual = self
                        .source_objects
                        .allocation_owner(&value)
                        .ok_or_else(|| {
                            fail("instanceof requires an exact source allocation identity")
                        })?;
                    let allocated = self
                        .program
                        .find_class(actual)
                        .ok_or_else(|| fail("instanceof allocation has no declared class"))?;
                    if let Some(target) = self.program.find_class(target) {
                        allocated.full_name() == target.full_name()
                            || crate::normalized::implements_specialization(
                                self.program,
                                allocated,
                                &JavaType::Class(target.full_name()),
                            )
                    } else {
                        need(
                            matches!(target.as_str(), "java.lang.Object" | "Object"),
                            "instanceof class is not declared",
                        )?;
                        true
                    }
                };
                Ok(Value::Term(Ty::Bool, b::lit_bool(present)))
            }
            J::Lit(JavaLit::Str(v)) => Ok(Value::Term(Ty::Text, b::lit_str(v))),
            J::Lit(JavaLit::StrUnits(units)) => Ok(Value::Term(
                Ty::Text,
                b::lit(math::Lit::from_text_units(units.clone())),
            )),
            // A char constant contributes its unsigned UTF-16 code unit to
            // a numeric expression, not a text value or reference identity.
            J::Lit(JavaLit::Char(v)) => Ok(Value::Term(Ty::Integer, b::lit_int(i64::from(*v)))),
            J::Lit(JavaLit::Null) => Ok(Value::Term(Ty::Ref, b::lit_null())),
            J::Lit(JavaLit::Bool(v)) => Ok(Value::Term(Ty::Bool, b::lit_bool(*v))),
            J::Lit(JavaLit::Decimal {
                digits,
                radix,
                power,
                ..
            }) => {
                let value = math::domains::decimal::from_radix_digits(digits, *radix, *power)
                    .expect("bounded numeric token is certified at source admission");
                Ok(Value::Term(Ty::Decimal, value))
            }
            J::Lit(JavaLit::Long(v)) => Ok(Value::Term(Ty::Integer, b::lit_int(*v))),
            J::Lit(JavaLit::Int(v)) => Ok(Value::Term(Ty::Integer, b::lit_int(i64::from(*v)))),
            J::New {
                java_type: JavaType::Class(owner),
                args,
            } => {
                let Some((args, path)) = self.source_arguments(args, id, env, path)? else {
                    return Ok(Value::Term(Ty::Ref, b::lit_null()));
                };
                self.construct_source_object(owner, args, path)
            }
            J::New {
                java_type: ty @ JavaType::Generic(..),
                args,
            } => {
                let proof = crate::normalized::generic_product::instantiated(self.program, ty)
                    .ok_or_else(|| {
                        fail("generic allocation requires concrete source constructor proof")
                    })?;
                let Some((evaluated, _)) = self.source_arguments(args, id, env, path)? else {
                    return Ok(Value::Term(Ty::Ref, b::lit_null()));
                };
                self.construct_generic_product(&proof, evaluated)
            }
            J::ArrayInitializer {
                element_type,
                elements,
            } => self
                .evaluate_local_array_initializer(element_type, elements, id, env, path)
                .map(Value::LocalIntArray),
            J::ArrayAllocation {
                element_type,
                length,
            } => self
                .evaluate_local_array_allocation(element_type, length, id, env, path)
                .map(Value::LocalIntArray),
            J::ArrayAccess { target, index } => {
                let Some((array, index, path)) =
                    self.source_binary_operands((target, index), id, env, path)?
                else {
                    return Ok(Value::Term(Ty::Integer, b::lit_null()));
                };
                self.source_array_read(array, index, path)
            }
            J::FieldAccess { target, field } => {
                if field == "length" {
                    if let Some(length) =
                        self.source_java_array_length(target, id, env, path.clone())?
                    {
                        return Ok(Value::Term(Ty::Integer, length));
                    }
                }
                self.source_field(target, field, id)?;
                if matches!(target.as_ref(), J::Ident(name) if name == "this")
                    && !env.contains_key("this")
                {
                    let owner = id.rsplit_once('.').unwrap().0;
                    if self.active_static_receiver.is_none() {
                        if let Some(value) = self.assembled_dependency(owner, field) {
                            return Ok(value);
                        }
                    }
                    return self.read_entry_state(owner, field, &path);
                }
                let reference = self.java_expr(target, id, env, path.clone())?;
                let path = self.normal_path(path);
                if path.as_bool() == Some(false) {
                    // No field value is acquired after evaluating a receiver
                    // that threw. This carrier cannot escape that path.
                    let field_type = self
                        .project
                        .parameter_type(&self.source_declared_type(j, id)?)?;
                    return Ok(Value::Term(field_type, b::lit_null()));
                }
                if matches!(reference.ty(), Ty::Record(_)) {
                    return values::field(reference, field);
                }
                let reference = reference.scalar()?;
                if let Some(value) = self.read_selected_field(&reference, field, &path)? {
                    return Ok(value);
                }
                if self
                    .source_objects
                    .has_complete_identity_partition(&reference)
                    || reference.is_null()
                {
                    let field_type = self
                        .project
                        .parameter_type(&self.source_declared_type(j, id)?)?;
                    return self.read_source_field_partition(&reference, field, field_type, path);
                }
                if self.source_objects.owns(&reference) {
                    let null = self.source_object_null(&reference);
                    self.throw_source_on_path(
                        b::and(path.clone(), null),
                        "java.lang.NullPointerException",
                    );
                    self.read_or_bind_source_field(&reference, field, &path)
                } else {
                    Err(fail("field read requires a closed source object"))
                }
            }
            J::MethodCall {
                target,
                method,
                args,
            } => {
                if target.is_none()
                    || matches!(target.as_deref(), Some(J::Ident(name)) if name == "this")
                {
                    let owner = id.rsplit_once('.').unwrap().0;
                    if let Some(interface) = self
                        .program
                        .find_class(owner)
                        .filter(|class| class.is_interface())
                    {
                        let (_, _, signature) = crate::normalized::interface_member(
                            self.program,
                            interface,
                            method,
                            args.len(),
                        )
                        .ok_or_else(|| fail("missing declared interface signature"))?;
                        let receiver = env
                            .get("this")
                            .ok_or_else(|| fail("interface default needs its bound receiver"))?
                            .scalar()?;
                        let Some((values, path)) = self.source_arguments(args, id, env, path)?
                        else {
                            return Ok(Value::Term(
                                self.project.declared_result_type(signature.return_type())?,
                                b::lit_null(),
                            ));
                        };
                        return self.invoke_interface(
                            owner,
                            &JavaType::Class(owner.into()),
                            method,
                            receiver,
                            values,
                            path,
                        );
                    }
                }
                if let Some(qualifier) = target
                    .as_deref()
                    .and_then(crate::normalized::interface_super_qualifier)
                {
                    let owner = id.rsplit_once('.').unwrap().0;
                    let (member, declaration) = self
                        .program
                        .find_class(owner)
                        .and_then(|caller| {
                            crate::normalized::interface_super_method(
                                self.program,
                                caller,
                                qualifier,
                                method,
                                args.len(),
                            )
                        })
                        .ok_or_else(|| fail("interface default target was not admitted"))?;
                    let receiver = env.get("this").cloned().ok_or_else(|| {
                        fail("qualified interface super needs the original receiver")
                    })?;
                    for ((expected, _), actual) in declaration.params().iter().zip(args) {
                        self.source_reference_assignment(expected, actual, id)?;
                    }
                    let Some((values, path)) = self.source_arguments(args, id, env, path)? else {
                        return Ok(Value::Term(
                            self.methods[&member].returns.clone(),
                            b::lit_null(),
                        ));
                    };
                    return self.java_call_on(&member, values, path, Some(receiver));
                }

                if !self.methods[id].syntax.is_static()
                    && !env.contains_key("this")
                    && self.active_static_receiver.is_none()
                {
                    if let Some(field) = target
                        .as_deref()
                        .and_then(crate::normalized::source_ports::receiver)
                        .filter(|field| {
                            matches!(target.as_deref(), Some(J::FieldAccess { .. }))
                                || !env.contains_key(*field)
                        })
                    {
                        let owner = id.rsplit_once('.').unwrap().0;
                        if let Some(receiver) = self.assembled_dependency(owner, field) {
                            let reference = receiver.scalar()?;
                            let (member, declaration) = self
                                .source_objects
                                .dispatch_owner(&reference)
                                .and_then(|class| self.program.find_class(class))
                                .zip(self.program.find_class(owner))
                                .and_then(|(dependency, caller)| {
                                    crate::normalized::assembled_instance_method(
                                        caller,
                                        dependency,
                                        method,
                                        args.len(),
                                    )
                                })
                                .ok_or_else(|| {
                                    fail("assembled receiver has no visible instance method")
                                })?;
                            for ((expected, _), actual) in declaration.params().iter().zip(args) {
                                self.source_reference_assignment(expected, actual, id)?;
                            }
                            let Some((values, path)) =
                                self.source_arguments(args, id, env, path)?
                            else {
                                return Ok(Value::Term(
                                    self.methods[&member].returns.clone(),
                                    b::lit_null(),
                                ));
                            };
                            return self.java_call_on(&member, values, path, Some(receiver));
                        }
                    }
                }
                if self.project.direct_source && self.source_root.is_some() {
                    let caller = id.rsplit_once('.').unwrap().0;
                    let candidate = match target.as_deref() {
                        Some(J::Ident(field)) if !env.contains_key(field) => {
                            self.program.find_class(caller).map(|owner| (owner, field))
                        }
                        Some(J::FieldAccess { target, field }) => match target.as_ref() {
                            J::Ident(owner) if owner != "this" && !env.contains_key(owner) => {
                                self.program.find_class(owner).map(|owner| (owner, field))
                            }
                            _ => None,
                        },
                        _ => None,
                    };
                    if let (Some(caller), Some((root, field))) =
                        (self.program.find_class(caller), candidate)
                    {
                        if let Some((owner, declaration, key)) = super::inert_instance_method(
                            self.program,
                            caller,
                            root,
                            field,
                            method,
                            args.len(),
                        ) {
                            let source = Path::new(root.source_file());
                            let path_from_root = source
                                .strip_prefix(self.source_root.unwrap())
                                .map_err(|_| "static receiver source outside project")?;
                            let locator = format!(
                                "{}#{}.{field}",
                                path_from_root.display(),
                                root.full_name()
                            );
                            let owner = owner.full_name();
                            for ((expected, _), actual) in declaration.params().iter().zip(args) {
                                self.source_reference_assignment(expected, actual, id)?;
                            }
                            let Some((values, path)) =
                                self.source_arguments(args, id, env, path)?
                            else {
                                return Ok(Value::Term(
                                    self.methods[&key].returns.clone(),
                                    b::lit_null(),
                                ));
                            };
                            let previous = self.active_static_receiver.replace((owner, locator));
                            let call = self.java_call_with_receiver(&key, values, path, None, true);
                            self.active_static_receiver = previous;
                            return call;
                        }
                    }
                }
                let owner = match target {
                    None => id.rsplit_once('.').unwrap().0,
                    Some(t) => crate::normalized::source_ports::receiver(t)
                        .ok_or_else(|| fail("Java method receiver must have a declared owner"))?,
                };
                if let Some(J::Ident(local)) = target.as_deref() {
                    if let Some(receiver) = env.get(local) {
                        let receiver_type =
                            self.source_declared_type(&J::Ident(local.clone()), id)?;
                        if let Some(interface) =
                            crate::normalized::specialized_interface(self.program, &receiver_type)
                        {
                            let receiver = receiver.scalar()?;
                            let Some((values, path)) =
                                self.source_arguments(args, id, env, path)?
                            else {
                                let (_, _, signature) = crate::normalized::interface_member(
                                    self.program,
                                    interface,
                                    method,
                                    args.len(),
                                )
                                .ok_or_else(|| fail("missing declared interface return type"))?;
                                let ty =
                                    self.project.declared_result_type(signature.return_type())?;
                                return Ok(Value::Term(ty, b::lit_null()));
                            };
                            return self.invoke_interface(
                                &interface.full_name(),
                                &receiver_type,
                                method,
                                receiver,
                                values,
                                path,
                            );
                        }
                        if let JavaType::Class(interface) = receiver_type {
                            if let Some((field, _)) =
                                self.program.find_class(&interface).and_then(|class| {
                                    crate::normalized::source_record_accessor(
                                        class,
                                        method,
                                        args.len(),
                                    )
                                })
                            {
                                return self.java_expr(
                                    &J::FieldAccess {
                                        target: Box::new(J::Ident(local.clone())),
                                        field: field.into(),
                                    },
                                    id,
                                    env,
                                    path,
                                );
                            }
                            // A fresh source allocation has an exact receiver.
                            // Execute its visible instance method on that same
                            // heap object, retaining the original this binding.
                            let caller = id.rsplit_once('.').unwrap().0;
                            if let Some((member, declaration)) = self
                                .program
                                .find_class(&interface)
                                .zip(self.program.find_class(caller))
                                .and_then(|(class, caller)| {
                                    crate::normalized::assembled_instance_method(
                                        caller,
                                        class,
                                        method,
                                        args.len(),
                                    )
                                })
                            {
                                if let Ok(reference) = receiver.scalar() {
                                    if reference.is_null()
                                        || self.source_objects.dispatch_owner(&reference)
                                            == Some(interface.as_str())
                                    {
                                        let receiver = receiver.clone();
                                        for ((expected, _), actual) in
                                            declaration.params().iter().zip(args)
                                        {
                                            self.source_reference_assignment(expected, actual, id)?;
                                        }
                                        let Some((values, path)) =
                                            self.source_arguments(args, id, env, path)?
                                        else {
                                            return Ok(Value::Term(
                                                self.methods[&member].returns.clone(),
                                                b::lit_null(),
                                            ));
                                        };
                                        return self.java_call_on(
                                            &member,
                                            values,
                                            path,
                                            Some(receiver),
                                        );
                                    }
                                }
                            }
                            // An ordinary input that shadows a marked receiver
                            // does not inherit the field's external contract.
                            if self.program.find_class(&interface).is_some_and(|class| {
                                crate::normalized::source_ports::declares_bodyless_member(
                                    class,
                                    method,
                                    args.len(),
                                )
                            }) && receiver.scalar().is_ok_and(|reference| {
                                reference.as_heap_ref().is_some()
                                    && self.source_objects.allocation_owner(&reference).is_none()
                            }) {
                                crate::preprocess::out_of_source(
                                    self.selected_entry.id,
                                    &format!("{interface}.{method}"),
                                );
                            }
                        }
                    }
                }
                let field_receiver = matches!(target.as_deref(), Some(J::FieldAccess { .. }));
                need(
                    field_receiver || !env.contains_key(owner),
                    "instance calls unsupported",
                )?;
                let callee = format!("{owner}.{method}");
                let port = format!("{}.{}", id.rsplit_once('.').unwrap().0, callee);
                if self.project.source_ports.contains_key(&port) {
                    need(
                        target.is_some() && !self.methods[id].syntax.is_static(),
                        "external instance field requires a bound receiver",
                    )?;
                    let rooted_identity = if self.project.direct_source {
                        let owner = id.rsplit_once('.').map(|(owner, _)| owner);
                        need(
                            !env.contains_key("this"),
                            "external port requires a statically bound receiver",
                        )?;
                        if let Some((receiver_owner, receiver)) = &self.active_static_receiver {
                            need(
                                Some(receiver_owner.as_str()) == owner,
                                "external port receiver owner mismatch",
                            )?;
                            let endpoint = &self.project.source_ports[&port].endpoint;
                            Some(format!("{receiver}.{endpoint}"))
                        } else {
                            let entry = self.selected_entry;
                            need(
                                !entry.syntax.is_static()
                                    && entry.id.rsplit_once('.').map(|(owner, _)| owner) == owner,
                                "external port requires the statically bound entry receiver",
                            )?;
                            None
                        }
                    } else {
                        None
                    };
                    let Some((values, path)) = self.source_arguments(args, id, env, path)? else {
                        let ty = self.checked_ports[&port].completion.returns.clone();
                        return Ok(Value::Term(ty, b::lit_null()));
                    };
                    return self.source_port_call(&port, rooted_identity, values, path);
                }
                need(
                    !field_receiver,
                    "field receiver is not a declared external port",
                )?;
                if self.project.direct_source
                    && self
                        .program
                        .find_class(id.rsplit_once('.').unwrap().0)
                        .is_some_and(|caller| {
                            crate::normalized_text::clock_intrinsic(
                                self.program,
                                caller,
                                &callee,
                                args.len(),
                            )
                        })
                {
                    return Ok(self.observe_clock(path));
                }
                use crate::normalized_decimal::{decimal_intrinsic, round_mode, DecimalIntrinsic};
                if let Some(intrinsic) = self
                    .program
                    .find_class(id.rsplit_once('.').unwrap().0)
                    .and_then(|caller| decimal_intrinsic(self.program, caller, &callee))
                {
                    let operand = |value: &Value| {
                        need(
                            value.ty() == Ty::Decimal,
                            "standard decimal operation requires exact decimal operands",
                        )?;
                        value.scalar()
                    };
                    use math::domains::decimal;
                    let result = match (intrinsic, args.as_slice()) {
                        (DecimalIntrinsic::Multiply, [_, _]) => {
                            let Some((values, _)) = self.source_arguments(args, id, env, path)?
                            else {
                                return Ok(Value::Term(Ty::Decimal, b::lit_null()));
                            };
                            decimal::multiply(operand(&values[0])?, operand(&values[1])?)
                        }
                        (DecimalIntrinsic::Round, [_, _, mode]) => {
                            let mode = round_mode(mode).ok_or_else(|| {
                                fail("N.round requires an explicit midpoint mode")
                            })?;
                            let Some((values, _)) =
                                self.source_arguments(&args[..2], id, env, path)?
                            else {
                                return Ok(Value::Term(Ty::Decimal, b::lit_null()));
                            };
                            need(
                                values[1].ty() == Ty::Integer,
                                "N.round requires an exact integer scale",
                            )?;
                            decimal::round_at(operand(&values[0])?, values[1].scalar()?, mode)
                        }
                        _ => return Err(fail("invalid standard decimal operation")),
                    };
                    return Ok(Value::Term(Ty::Decimal, math::normalize(result)));
                }
                let text_intrinsic = self
                    .program
                    .find_class(id.rsplit_once('.').unwrap().0)
                    .and_then(|caller| {
                        crate::normalized_text::source_intrinsic(
                            self.program,
                            caller,
                            &callee,
                            args.len(),
                        )
                    });
                let resolved = if text_intrinsic.is_none() && self.project.direct_source {
                    self.program
                        .find_class(owner)
                        .and_then(|class| {
                            crate::normalized::named_method_for_arity(class, method, args.len())
                        })
                        .map(|(key, _)| key)
                        .unwrap_or(callee)
                } else {
                    callee
                };
                if let Some(declaration) = self
                    .methods
                    .get(&resolved)
                    .filter(|_| text_intrinsic.is_none())
                {
                    for ((expected, _), actual) in declaration.syntax.params().iter().zip(args) {
                        self.source_reference_assignment(expected, actual, id)?;
                    }
                }
                let Some((vs, path)) = self.source_arguments(args, id, env, path)? else {
                    let ty = if text_intrinsic.is_some() {
                        Ty::Text
                    } else {
                        self.methods
                            .get(&resolved)
                            .ok_or_else(|| {
                                fail("missing declared return type for unreachable call")
                            })?
                            .returns
                            .clone()
                    };
                    return Ok(Value::Term(ty, b::lit_null()));
                };
                if let Some(intrinsic) = text_intrinsic {
                    need(
                        vs.iter().all(|v| {
                            v.ty() == Ty::Text || v.scalar().is_ok_and(|expr| expr.is_null())
                        }),
                        "text intrinsic requires text arguments",
                    )?;
                    let value = vs[0].scalar()?;
                    use crate::normalized_text::SourceIntrinsic;
                    let expr = match intrinsic {
                        SourceIntrinsic::Trim => crate::normalized_text::trim(value),
                        SourceIntrinsic::DefaultIfBlank => {
                            crate::normalized_text::default_if_blank(value, vs[1].scalar()?)
                        }
                    };
                    return Ok(Value::Term(Ty::Text, expr));
                }
                if target.is_none() && !self.methods[id].syntax.is_static() {
                    return self.java_call_with_receiver(
                        &resolved,
                        vs,
                        path,
                        env.get("this").cloned(),
                        true,
                    );
                }
                self.java_call(&resolved, vs, path)
            }
            J::UnaryOp {
                op: U::Not,
                operand,
            } => {
                let v = self.java_expr(operand, id, env, path.clone())?;
                if self.normal_path(path).as_bool() == Some(false) {
                    return Ok(Value::Term(Ty::Bool, b::lit_null()));
                }
                need(v.ty() == Ty::Bool, "not requires bool")?;
                Ok(Value::Term(Ty::Bool, b::not(v.scalar()?)))
            }
            // Source admission only accepts the integral widening cast.
            J::Cast { operand, .. } => {
                let operand = self.java_expr(operand, id, env, path.clone())?;
                if self.normal_path(path).as_bool() == Some(false) {
                    return Ok(Value::Term(Ty::Integer, b::lit_null()));
                }
                need(
                    operand.ty() == Ty::Integer,
                    "widening cast requires an integer",
                )?;
                Ok(operand)
            }
            J::UnaryOp {
                op: U::Neg,
                operand,
            } => {
                let value = self.java_expr(operand, id, env, path.clone())?;
                if self.normal_path(path).as_bool() == Some(false) {
                    return Ok(value);
                }
                if value.ty() == Ty::Decimal {
                    return Ok(Value::Term(
                        Ty::Decimal,
                        math::domains::decimal::negate(value.scalar()?),
                    ));
                }
                need(value.ty() == Ty::Integer, "negation requires an integer")?;
                Ok(Value::Term(
                    Ty::Integer,
                    self.normalize_execution_value(b::neg(value.scalar()?)),
                ))
            }
            J::BinOp {
                op: B::Add,
                left,
                right,
            } => self.java_arithmetic(Arithmetic::Add, (left, right), id, env, path),
            J::BinOp {
                op: B::Sub,
                left,
                right,
            } => self.java_arithmetic(Arithmetic::Sub, (left, right), id, env, path),
            J::BinOp {
                op: B::Mul,
                left,
                right,
            } => self.java_arithmetic(Arithmetic::Mul, (left, right), id, env, path),
            J::BinOp {
                op: op @ (B::Div | B::Mod),
                left,
                right,
            } => {
                let no_value = || Value::Term(Ty::Integer, b::lit_null());
                let Some((left, right, normal)) =
                    self.source_binary_operands((left, right), id, env, path)?
                else {
                    return Ok(no_value());
                };
                need(
                    left.ty() == Ty::Integer && right.ty() == Ty::Integer,
                    "division and remainder require integer operands",
                )?;
                let (left, right) = (
                    math::normalize(left.scalar()?),
                    math::normalize(right.scalar()?),
                );
                let zero = self.condition_under_path(b::eq(right.clone(), b::lit_int(0)), &normal);
                if zero.as_bool() != Some(false) {
                    // Register the actual Java completion before simplifying
                    // the normal domain. A caught divide-by-zero is observable.
                    self.throw_source_on_path(
                        b::and(normal, zero.clone()),
                        "java.lang.ArithmeticException",
                    );
                }
                if zero.as_bool() == Some(true) {
                    return Ok(no_value());
                }
                if left == right {
                    return Ok(Value::Term(
                        Ty::Integer,
                        b::lit_int(if *op == B::Div { 1 } else { 0 }),
                    ));
                }
                if *op == B::Div {
                    return Ok(Value::Term(
                        Ty::Integer,
                        math::domains::integer_quotient::on_nonzero_path(left, right),
                    ));
                }
                Ok(Value::Term(
                    Ty::Integer,
                    math::domains::integer_quotient::remainder_on_nonzero_path(left, right),
                ))
            }
            J::BinOp { op, left, right } if matches!(op, B::And | B::Or) => {
                let left = self.java_expr(left, id, env, path.clone())?;
                let path = self.normal_path(path);
                if path.as_bool() == Some(false) {
                    return Ok(Value::Term(Ty::Bool, b::lit_null()));
                }
                need(
                    left.ty() == Ty::Bool,
                    "Java short-circuit left operand must be boolean",
                )?;
                let condition = self.condition_under_path(left.scalar()?, &path);
                let on_right = if *op == B::And {
                    condition.clone()
                } else {
                    b::not(condition.clone())
                };
                let right_path = self.normal_path(b::and(path.clone(), on_right));
                let short_value = || Value::Term(Ty::Bool, b::lit_bool(*op == B::Or));
                if right_path.as_bool() == Some(false) {
                    return Ok(short_value());
                }
                let right = self.java_expr(right, id, env, right_path.clone())?;
                if self.normal_path(right_path).as_bool() == Some(false) {
                    return Ok(short_value());
                }
                need(
                    right.ty() == Ty::Bool,
                    "Java short-circuit right operand must be boolean",
                )?;
                if *op == B::And {
                    self.choose(condition, right, Value::Term(Ty::Bool, b::lit_bool(false)))
                } else {
                    self.choose(condition, Value::Term(Ty::Bool, b::lit_bool(true)), right)
                }
            }
            J::Ternary {
                cond,
                then_expr,
                else_expr,
            } => {
                let condition = self.java_expr(cond, id, env, path.clone())?;
                let path = self.normal_path(path);
                if path.as_bool() == Some(false) {
                    return Ok(Value::Term(Ty::Unit, b::lit_null()));
                }
                let condition = self.condition_under_path(condition.scalar()?, &path);
                if let Some(taken) = condition.as_bool() {
                    return self.java_expr(
                        if taken { then_expr } else { else_expr },
                        id,
                        env,
                        path,
                    );
                }
                let yes_path = self.normal_path(b::and(path.clone(), condition.clone()));
                let no_path = self.normal_path(b::and(path, b::not(condition.clone())));
                let yes = self.source_operand(then_expr, id, env, yes_path)?;
                let no = self.source_operand(else_expr, id, env, no_path)?;
                let (yes, no) = match (yes, no) {
                    (Some((yes, _)), Some((no, _))) => (yes, no),
                    (Some((value, _)), None) | (None, Some((value, _))) => return Ok(value),
                    (None, None) => return Ok(Value::Term(Ty::Unit, b::lit_null())),
                };
                let (yes, no) = if matches!(
                    (yes.ty(), no.ty()),
                    (Ty::Integer, Ty::Decimal) | (Ty::Decimal, Ty::Integer)
                ) {
                    (
                        Self::promote_native_decimal(yes),
                        Self::promote_native_decimal(no),
                    )
                } else {
                    (yes, no)
                };
                self.choose(condition, yes, no)
            }
            J::BinOp { op, left, right } => {
                use math::domains::decimal::Operator;
                let comparison = match op {
                    B::Eq => Operator::Eq,
                    B::Ne => Operator::Ne,
                    B::Lt => Operator::Lt,
                    B::Le => Operator::Le,
                    B::Gt => Operator::Gt,
                    B::Ge => Operator::Ge,
                    _ => return Err(fail("unsupported Java expression in pure-array-1")),
                };
                let op = comparison.binop();
                let nullable = match (left.as_ref(), right.as_ref()) {
                    (value, J::Lit(JavaLit::Null)) | (J::Lit(JavaLit::Null), value)
                        if matches!(op, BinOpKind::Eq | BinOpKind::Ne) =>
                    {
                        Some(value)
                    }
                    _ => None,
                };
                if let Some(operand) = nullable {
                    let v = self.java_expr(operand, id, env, path.clone())?;
                    if self.normal_path(path).as_bool() == Some(false) {
                        return Ok(Value::Term(Ty::Bool, b::lit_null()));
                    }
                    if let Value::Term(Ty::Text, expression) = &v {
                        if let Some(test) =
                            self.compare_first_text_to_null(expression, op == BinOpKind::Ne)
                        {
                            return Ok(Value::Term(Ty::Bool, test));
                        }
                    }
                    if matches!(&v, Value::LocalIntArray(_)) {
                        return Ok(Value::Term(Ty::Bool, b::lit_bool(op == BinOpKind::Ne)));
                    }
                    if let Value::Sequence { null, .. } = &v {
                        let predicate = if op == BinOpKind::Ne {
                            b::not(null.clone())
                        } else {
                            null.clone()
                        };
                        return Ok(Value::Term(
                            Ty::Bool,
                            self.normalize_execution_value(predicate),
                        ));
                    }
                    let presence = match &v {
                        Value::Term(Ty::Ref, r) => self.row_presence(r),
                        _ => None,
                    };
                    let null = if let Some(present) = presence {
                        b::not(present)
                    } else if matches!(&v, Value::Term(Ty::Ref, reference)
                        if self.source_objects.owns(reference) || self.source_objects.has_complete_identity_partition(reference))
                    {
                        self.source_reference_equality(v.scalar()?, b::lit_null(), false)?
                    } else {
                        match v {
                            Value::Term(Ty::Ref | Ty::Text, e) => b::eq(e, b::lit_null()),
                            _ => return Err(fail("null comparison type")),
                        }
                    };
                    return Ok(Value::Term(
                        Ty::Bool,
                        if op == BinOpKind::Ne {
                            b::not(null)
                        } else {
                            null
                        },
                    ));
                }
                let Some((l, r, path)) =
                    self.source_binary_operands((left, right), id, env, path)?
                else {
                    return Ok(Value::Term(Ty::Bool, b::lit_null()));
                };
                need(
                    l.ty() != Ty::Text,
                    "Java text identity comparison requires explicit identity representation",
                )?;
                if matches!(
                    (l.ty(), r.ty()),
                    (Ty::Decimal, Ty::Decimal)
                        | (Ty::Decimal, Ty::Integer)
                        | (Ty::Integer, Ty::Decimal)
                ) {
                    return Ok(Value::Term(
                        Ty::Bool,
                        math::domains::decimal::binary(comparison, l.scalar()?, r.scalar()?),
                    ));
                }
                need(l.ty() == r.ty(), "Java comparison type mismatch")?;
                let ty = l.ty();
                need(
                    matches!(op, BinOpKind::Eq | BinOpKind::Ne) || matches!(ty, Ty::Integer),
                    "ordered comparison type",
                )?;
                if matches!(l, Value::IntegerFloor { .. })
                    || matches!(r, Value::IntegerFloor { .. })
                {
                    let domain = self.normal_path(path);
                    if let Some(value) = compare_integer_floor(op, &l, &r, &domain) {
                        return Ok(Value::Term(Ty::Bool, b::lit_bool(value)));
                    }
                }
                let identity = match (&l, &r) {
                    (Value::LocalIntArray(left), Value::LocalIntArray(right)) => {
                        Some(self.local_array_identity_eq(left, right)?)
                    }
                    (
                        Value::Sequence {
                            item: Ty::Integer,
                            name: left,
                            ..
                        },
                        Value::Sequence {
                            item: Ty::Integer,
                            name: right,
                            ..
                        },
                    ) => Some(self.borrowed_array_identity_eq(left, right)?),
                    _ => None,
                };
                if let Some(same) = identity {
                    let value = if op == BinOpKind::Ne {
                        self.normalize_execution_value(b::not(same))
                    } else {
                        same
                    };
                    return Ok(Value::Term(Ty::Bool, value));
                }
                let a = l.scalar()?;
                let z = r.scalar()?;
                need(
                    !matches!(ty, Ty::Record(_)),
                    "member choice record has content but no Java reference identity",
                )?;
                if ty == Ty::Ref {
                    let identity = self.source_reference_equality(a, z, op == BinOpKind::Ne)?;
                    return Ok(Value::Term(Ty::Bool, identity));
                }
                Ok(Value::Term(
                    Ty::Bool,
                    self.order_execution_comparison(op, a, z),
                ))
            }
            _ => Err(fail("unsupported Java expression in pure-array-1")),
        }
    }
}

// Apply the source host's proof before entering the unconstrained math cache.
fn compare_integer_floor(
    op: BinOpKind,
    left: &Value,
    right: &Value,
    domain: &Expr,
) -> Option<bool> {
    fn against_literal(
        op: BinOpKind,
        subject: &Value,
        bound: &Value,
        domain: &Expr,
    ) -> Option<bool> {
        let lower = subject.certified_integer_floor(domain)?;
        let Value::Term(Ty::Integer, bound) = bound else {
            return None;
        };
        let bound = bound.as_lit()?.as_integer()?;
        match op {
            BinOpKind::Lt if lower >= bound => Some(false),
            BinOpKind::Le | BinOpKind::Eq if lower > bound => Some(false),
            BinOpKind::Gt | BinOpKind::Ne if lower > bound => Some(true),
            BinOpKind::Ge if lower >= bound => Some(true),
            _ => None,
        }
    }
    against_literal(op, left, right, domain)
        .or_else(|| against_literal(op.swapped_comparison()?, right, left, domain))
}

impl Engine<'_> {
    pub(super) fn has_observable_boundary(&self) -> bool {
        !self.project.heap_fields.is_empty()
            || !self.project.ports.is_empty()
            || !self.project.exceptions.is_empty()
    }

    pub(super) fn profile_metrics(&self, status: &str) -> String {
        format!(
            "phase=kernel_execution status={status} steps={} semantic_calls={} java_calls={} node_visits={} branch_splits={} branch_collapses={} max_active_calls={} active_calls={} observable_boundary={} node_depth={} max_node_depth={}",
            self.steps,
            self.semantic_calls,
            self.java_calls,
            self.node_visits,
            self.branch_splits,
            self.branch_collapses,
            self.max_active_calls,
            self.active.len(),
            self.has_observable_boundary(),
            self.node_depth,
            self.max_node_depth,
        )
    }
    pub(super) fn emit_stage_profile(&self, stage: &str, started: Instant, status: &str) {
        if crate::profile::enabled() {
            crate::profile::event(
                "kernel_execution.stage",
                &format!(
                    "stage={stage} elapsed_us={} {}",
                    started.elapsed().as_micros(),
                    self.profile_metrics(status),
                ),
            );
        }
    }
    pub(super) fn emit_profile_metrics(&self, status: &str) {
        if crate::profile::enabled() {
            crate::profile::event("kernel_execution.metrics", &self.profile_metrics(status));
        }
    }
    pub(super) fn condition_under_path(&self, condition: Expr, path: &Expr) -> Expr {
        let condition = self.normalize_execution_value(condition);
        let path = math::normalize(self.normal_path(path.clone()));
        if b::path_implies(&path, &condition) {
            b::lit_bool(true)
        } else if b::path_implies(&path, &math::normalize(b::not(condition.clone()))) {
            b::lit_bool(false)
        } else {
            condition
        }
    }
    fn acquired_integer_receipt(&self, value: &Expr, path: &Expr) -> bool {
        enum Observation<'a> {
            Scalar(u64),
            CompletedScalar(&'a Expr),
            Field(u64, &'a str),
            ProjectedField(&'a Expr, &'a str),
        }

        // The caller already established exact integer operand types.
        let observation = if let Some(SymVarOrigin::External {
            binding_id, path, ..
        }) = value.as_sym_var()
        {
            match path.as_slice() {
                [] => Observation::Scalar(*binding_id),
                [field] => Observation::Field(*binding_id, field),
                _ => return false,
            }
        } else if value.as_call().is_some_and(|(key, _)| {
            math::domains::completion::output_name(&math::func_name(key)) == "completion.value"
        }) {
            Observation::CompletedScalar(value)
        } else if let Some((record, field, math::Primitive::Number)) =
            math::domains::composite::field_projection(value)
        {
            Observation::ProjectedField(record, field)
        } else {
            return false;
        };

        let path = self.normal_path(path.clone());
        self.realtime_effects.events().iter().any(|event| {
            if let realtime_state::EffectOperation::Acquire {
                acquisition: realtime_state::Acquisition::Clock,
                binding_id,
            } = &event.operation
            {
                return matches!(observation, Observation::Scalar(binding) if binding == *binding_id)
                    && b::path_implies(&path, &event.guard);
            }
            let realtime_state::EffectOperation::Port {
                receipt,
                completion: spec,
                ..
            } = &event.operation
            else {
                return false;
            };
            // A bare integer is only minted for a nonthrowing source port.
            // For fields, prove the declared integer and the exact response
            // before attributing that observation to this occurrence.
            let field = match observation {
                Observation::Scalar(binding) => {
                    return receipt.binding_id() == binding && b::path_implies(&path, &event.guard)
                }
                Observation::CompletedScalar(value) => {
                    let response = receipt.response();
                    let acquired = b::and(event.guard.clone(), spec.normal_condition(response));
                    return spec.returns == Ty::Integer
                        && value == &spec.normal_value(response)
                        && b::path_implies(&path, &acquired);
                }
                Observation::Field(binding, field) if receipt.binding_id() == binding => field,
                Observation::ProjectedField(_, field) => field,
                _ => return false,
            };
            let Ty::Record(fields) = &spec.returns else {
                return false;
            };
            if fields.get(field) != Some(&Ty::Integer) {
                return false;
            }
            let response = receipt.response();
            match observation {
                // A direct field with this receipt identity was issued from
                // its bare completion root. Throwing ports project from
                // completion.value instead; external state has another ID.
                Observation::Field(..) => b::path_implies(&path, &event.guard),
                Observation::ProjectedField(record, _)
                    if record == &spec.normal_value(response) =>
                {
                    let acquired = b::and(event.guard.clone(), spec.normal_condition(response));
                    b::path_implies(&path, &acquired)
                }
                _ => false,
            }
        })
    }

    pub(super) fn throw_source_on_path(&mut self, path: Expr, exception: &str) {
        let path = math::normalize(self.normal_path(path));
        if path.as_bool() != Some(false) {
            self.realtime_completion =
                self.realtime_completion
                    .replace_continue_with_throw(path.clone(), exception, None);
            self.realtime_throws.push(path);
        }
    }

    pub(super) fn acquire_source_state_root(&mut self, key: String) -> Expr {
        let id = *self
            .state_roots
            .entry(key)
            .or_insert_with(|| self.heap.new_path_object().id());
        b::heap_ref(id)
    }

    pub(super) fn normal_path(&self, path: Expr) -> Expr {
        self.realtime_throws.iter().fold(path, |p, thrown| {
            math::normalize(b::and(p, b::not(thrown.clone())))
        })
    }
    pub(super) fn budget(&mut self) -> Result<()> {
        // A finite source induction can still exceed the work budget. Never
        // publish a partial execution or treat the limit as a business exit.
        const MAX_EXECUTION_STEPS: usize = 1_000_000;
        if self.steps >= MAX_EXECUTION_STEPS {
            return Err(format!(
                "[NORM-ANALYSIS-INCOMPLETE] execution step budget exhausted at {}",
                self.diagnostic_entry()
            )
            .into());
        }
        self.steps += 1;
        Ok(())
    }

    fn invoke_interface(
        &mut self,
        interface: &str,
        receiver_type: &JavaType,
        member: &str,
        receiver: Expr,
        args: Vec<Value>,
        path: Expr,
    ) -> Result<Value> {
        self.budget()?;
        if let Some(branches) = receiver.as_case_when() {
            let mut remaining = b::lit_bool(true);
            let mut values = Vec::new();
            let mut ty = None;
            // Exact receivers are evaluated only on their own live path.
            // This first dispatcher admits pure override branches, not a
            // speculative evaluation of incompatible effectful alternatives.
            for (condition, selected) in branches {
                let (guard, later) = b::first_match_domains(remaining, condition);
                let branch = math::normalize(b::and(path.clone(), guard.clone()));
                remaining = later;
                if branch.as_bool() == Some(false) {
                    continue;
                }
                let effects = self.realtime_effects.events().len();
                let throws = self.realtime_throws.len();
                let completion = self.realtime_completion.clone();
                let value = self.invoke_interface(
                    interface,
                    receiver_type,
                    member,
                    selected.clone(),
                    args.clone(),
                    branch,
                )?;
                need(effects == self.realtime_effects.events().len() && throws == self.realtime_throws.len()
                    && completion == self.realtime_completion, "conditional interface dispatch requires total overrides without external effects")?;
                need(
                    ty.as_ref().is_none_or(|t| *t == value.ty()),
                    "interface return types differ",
                )?;
                ty = Some(value.ty());
                values.push((guard, value.scalar()?));
            }
            need(
                remaining.as_bool() == Some(false),
                "interface receiver alternatives do not cover the source condition",
            )?;
            let ty = ty.ok_or_else(|| fail("no live interface receiver"))?;
            let (_, fallback) = values
                .pop()
                .ok_or_else(|| fail("no live interface receiver"))?;
            return Ok(Value::Term(
                ty,
                math::normalize(b::case_when(values, fallback)),
            ));
        }
        need(
            !receiver.is_null(),
            "null interface receiver requires a closed null-completion contract",
        )?;
        let target = format!("{interface}.{member}");
        let owner = self
            .source_objects
            .allocation_owner(&receiver)
            .unwrap_or_else(|| crate::preprocess::out_of_source(self.diagnostic_entry(), &target));
        let class = self
            .program
            .find_class(owner)
            .ok_or_else(|| fail("missing selected implementation"))?;
        need(
            crate::normalized::implements_specialization(self.program, class, receiver_type),
            "selected receiver does not implement declared interface specialization",
        )?;
        let declaration = self
            .program
            .find_class(interface)
            .ok_or_else(|| fail("missing selected interface"))?;
        let (_, _, signature) =
            crate::normalized::interface_member(self.program, declaration, member, args.len())
                .ok_or_else(|| fail("missing or ambiguous interface signature"))?;
        let actual = crate::normalized::selected_interface_method(self.program, class, signature)
            .ok_or_else(|| fail("missing proven receiver override or default body"))?;
        self.java_call_on(&actual, args, path, Some(Value::Term(Ty::Ref, receiver)))
    }

    pub(super) fn java_call(&mut self, id: &str, args: Vec<Value>, path: Expr) -> Result<Value> {
        self.java_call_on(id, args, path, None)
    }
    pub(super) fn java_call_on(
        &mut self,
        id: &str,
        args: Vec<Value>,
        path: Expr,
        receiver: Option<Value>,
    ) -> Result<Value> {
        self.java_call_with_receiver(id, args, path, receiver, false)
    }

    /// The linked entry already proved its statically assembled receiver.
    /// Entry-state reads use the initial snapshot, not constructor constants.
    pub(super) fn java_call_on_certified_entry_receiver(
        &mut self,
        id: &str,
        args: Vec<Value>,
        path: Expr,
    ) -> Result<Value> {
        self.java_call_with_receiver(id, args, path, None, true)
    }

    fn java_call_with_receiver(
        &mut self,
        id: &str,
        mut args: Vec<Value>,
        path: Expr,
        receiver: Option<Value>,
        implicit_receiver: bool,
    ) -> Result<Value> {
        self.budget()?;
        if implicit_receiver
            && self
                .source_assembly
                .is_some_and(|r| r.startup_setters.contains(id))
        {
            return Err(fail(
                "initial assembly setter is not a live receiver mutation contract",
            ));
        }
        let method = self
            .methods
            .get(id)
            .unwrap_or_else(|| crate::preprocess::out_of_source(self.diagnostic_entry(), id));
        let m = method.syntax;
        need(m.params().len() == args.len(), "Java arity mismatch")?;
        let parameter_types = method.parameter_types();
        for (((t, _), v), expected) in m.params().iter().zip(&mut args).zip(parameter_types) {
            if crate::normalized::native_decimal_type(t) {
                if v.ty() == Ty::Integer {
                    *v = Self::promote_native_decimal(v.clone());
                }
            }
            if *expected == Ty::Text
                && matches!(v, Value::Term(Ty::Ref, e) if e.as_lit() == Some(&math::Lit::Null))
            {
                *v = Value::Term(Ty::Text, b::lit_null());
            }
            need(*expected == v.ty(), "Java argument type mismatch")?;
        }
        // Receiver acquisition and argument evaluation happened in the
        // caller. A null instance receiver ends that invocation before
        // creating a callee frame, even for an otherwise recursive target.
        if !m.is_static()
            && matches!(&receiver, Some(Value::Term(Ty::Ref, reference)) if reference.is_null())
        {
            let returns = method.returns.clone();
            self.throw_source_on_path(path, "java.lang.NullPointerException");
            return Ok(Value::Term(returns, b::lit_null()));
        }
        self.java_calls += 1;
        let parent_completion = (!self.active.is_empty()).then(|| self.realtime_completion.clone());
        need(
            self.active.insert(format!("java:{id}")),
            "recursive Java call",
        )?;
        self.max_active_calls = self.max_active_calls.max(self.active.len());
        let value = if let Some(binding) = method.binding.clone() {
            self.call(id, &binding, args, path)?
        } else if let Some(proof) = crate::normalized::affine_recursion::prove(m) {
            let input = args.pop().expect("unary affine recursion").scalar()?;
            let base = b::lit_integer(proof.base);
            let boundary = b::lit_integer(proof.boundary);
            let affine = math::normalize(b::add(
                base.clone(),
                b::mul(
                    b::sub(input.clone(), boundary.clone()),
                    b::lit_integer(proof.slope),
                ),
            ));
            let positive = self.condition_under_path(b::gt(input, boundary), &path);
            let value = math::normalize(b::case_when(vec![(positive, affine)], base));
            self.realtime_completion = realtime_state::Completion::Return(value.clone());
            Value::Term(Ty::Integer, value)
        } else {
            need(
                m.is_static()
                    || implicit_receiver
                    || receiver.is_some()
                    || self.project.entries.contains(&id.to_string()),
                "instance method requires its bound receiver",
            )?;
            let mut env: Env = m
                .params()
                .iter()
                .zip(args)
                .map(|((_, n), v)| (n.clone(), v))
                .collect();
            if let Some(receiver) = receiver {
                env.insert("this".into(), receiver);
            }
            let body = m
                .body()
                .unwrap_or_else(|| crate::preprocess::out_of_source(self.diagnostic_entry(), id));
            let value = self.block(body, id, &mut env, path)?;
            need(
                value.ty() == self.methods[id].returns,
                "Java return type mismatch",
            )?;
            value
        };
        if let Some(parent) = parent_completion {
            self.realtime_completion =
                parent.replace_continue(self.realtime_completion.resume_after_normal_return());
        }
        self.active.remove(&format!("java:{id}"));
        Ok(value)
    }
    fn call(
        &mut self,
        id: &str,
        binding: &CheckedBinding<'_>,
        args: Vec<Value>,
        path: Expr,
    ) -> Result<Value> {
        self.budget()?;
        self.semantic_calls += 1;
        let name = binding.name;
        // Adapter nodes cannot enter another method. Retain this key only
        // for the existing call-budget and profiling accounting.
        self.active.insert(format!("semantic:{name}"));
        let f = binding.function;
        let mut env = f
            .params
            .iter()
            .zip(args)
            .map(|(p, v)| (p.name.clone(), v))
            .collect();
        self.max_active_calls = self.max_active_calls.max(self.active.len());
        let value = self.node(&f.body, &mut env, path).map_err(|error| {
            // Retain the original failure and add the reachable source chain.
            // This is diagnostic context only, never a fallback or retry.
            let context = format!(
                "\ncontext=semantic:{name} java={} source={}",
                f.java, f.origin.path
            );
            error.replace(
                "\nresult=not-produced",
                &(context + "\nresult=not-produced"),
            )
        })?;
        need(
            value.ty() == self.methods[id].returns,
            "semantic result type mismatch",
        )?;
        self.active.remove(&format!("semantic:{name}"));
        Ok(value)
    }
    pub(super) fn node(&mut self, n: &Node, env: &mut Env, path: Expr) -> Result<Value> {
        self.node_depth += 1;
        self.max_node_depth = self.max_node_depth.max(self.node_depth);
        let result = self.node_inner(n, env, path);
        self.node_depth -= 1;
        result
    }
}
