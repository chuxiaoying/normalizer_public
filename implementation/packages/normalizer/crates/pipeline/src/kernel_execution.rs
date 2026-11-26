use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum JavaArithmetic {
    Add,
    Sub,
    Mul,
}

impl JavaArithmetic {
    fn build(self, left: Expr, right: Expr) -> Expr {
        match self {
            Self::Add => b::add(left, right),
            Self::Sub => b::sub(left, right),
            Self::Mul => b::mul(left, right),
        }
    }
}


pub(super) struct Engine<'a> {
    pub(super) program: &'a JavaProgram,
    pub(super) temporal_definitions: Vec<super::temporal_graph::Definition>,
    pub(super) source_relations:super::business_relation::Relations,
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
    pub(super) action_can_await: bool,
    pub(super) selected_entry: Option<&'a str>,
    pub(super) entry_context: Option<&'a business_entry::EntryContext>,
    pub(super) state_roots: BTreeMap<String, i64>,
    pub(super) checked_ports: BTreeMap<String, actions::CheckedPort>,
    pub(super) pending_ports: BTreeMap<u64, String>,
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
    pub(super) budget_reported: bool,
}

impl Engine<'_> {
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
            Node::ObserveTuple { tuple, observations, body } => {
                need(super::tasks::expression_is_closed(body), "tuple observation requires a pure scalar body")?;
                self.observe_tuple(tuple, observations, body, env, path)
            }
            Node::OrderedTextMap { entries } => {
                let acquired = entries.iter().map(|entry| {
                    let value = env.get(&entry.input).ok_or_else(|| fail("unbound readonly Map entry"))?;
                    need(value.ty() == Ty::Text, "readonly Map entry requires an acquired nullable text parameter")?;
                    Ok((entry.key.clone(), value.scalar()?))
                }).collect::<Result<Vec<_>>>()?;
                let map = math::domains::composite::lifted_ordered_text_map(&acquired).map_err(fail)?;
                Ok(Value::Term(Ty::OrderedTextMap, map))
            }
            Node::NumberVector { items } => {
                let mut values = Vec::with_capacity(items.len());
                for item in items {
                    let value = self.node(item, env, path.clone())?;
                    need(value.ty() == Ty::Integer, "readonly Vector literal requires numbers")?;
                    values.push(value.scalar()?);
                }
                values::number_vector(values)
            }
            Node::Field { value, name } => {
                let acquired = self.node(value, env, path)?;
                if name == "length" && matches!(&acquired, Value::Term(Ty::Vector(_), _)) {
                    let input = acquired.scalar()?;
                    let binding = math::domains::composite::input_name(&input)
                        .ok_or_else(|| fail("length requires a direct readonly Vector input"))?;
                    let entry = self.selected_entry.and_then(|full| full.rsplit('.').next())
                        .ok_or_else(|| fail("length requires a declared entry"))?;
                    need(acquired.ty() == Ty::Vector(Box::new(Ty::Integer))
                        && self.project.parameter_value_types.get(entry).and_then(|params| params.get(binding))
                            .is_some_and(|ty| ty == "vector<number>"),
                        "length requires a declared non-null readonly numeric Vector")?;
                    Ok(Value::Term(Ty::Integer, math::domains::composite::certified_vector_length(input)))
                } else {
                    values::field(acquired, name)
                }
            }
            Node::Settled { returns, steps } => {
                tasks::validate_settled_scope(steps)?;
                self.action_scope(returns, steps, env, path, true)
            }
            Node::Action { returns, steps } => self.action_scope(returns, steps, env, path, false),
            Node::Add { left, right } | Node::Subtract { left, right } => {
                let left = self.node(left, env, path.clone())?;
                let right = self.node(right, env, path)?;
                need(left.ty() == Ty::Integer && right.ty() == Ty::Integer, "exact arithmetic requires business numbers")?;
                let operation = if matches!(n, Node::Subtract { .. }) { b::sub } else { b::add };
                Ok(Value::Term(Ty::Integer, math::normalize(operation(left.scalar()?, right.scalar()?))))
            }
            Node::Var { name } => env
                .get(name)
                .cloned()
                .ok_or_else(|| fail(format!("unknown variable {name}"))),
            Node::Const { typ, value } => {
                let t = Ty::parse(typ)?;
                let e = match (&t, value) {
                    (Ty::Unit, serde_json::Value::Null) => b::lit_null(),
                    (Ty::Bool, serde_json::Value::Bool(v)) => b::lit_bool(*v),
                    (Ty::Integer, serde_json::Value::Number(v)) => {
                        let x = v.as_i64().ok_or_else(|| fail("integer range"))?;
                        b::lit_int(x)
                    }
                    (Ty::Ref | Ty::Text, serde_json::Value::Null) => b::lit_null(),
                    (Ty::Text, serde_json::Value::String(v)) => b::lit_str(v),
                    _ => return Err(fail("invalid constant")),
                };
                Ok(Value::Term(t, e))
            }
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
            Node::Eq { left, right } | Node::Gt { left, right } => {
                let l = self.node(left, env, path.clone())?;
                let r = self.node(right, env, path)?;
                need(l.ty() == r.ty(), "binary type mismatch")?;
                need(l.ty() != Ty::Text, "text content cannot substitute reference identity; define the required comparison")?;
                need(
                    matches!(l.ty(), Ty::Bool | Ty::Integer | Ty::Ref),
                    "composite equality is not declared",
                )?;
                let gt = matches!(n, Node::Gt { .. });
                need(
                    !gt || matches!(l.ty(), Ty::Integer),
                    "gt requires integer",
                )?;
                Ok(Value::Term(
                    Ty::Bool,
                    if gt {
                        b::gt(l.scalar()?, r.scalar()?)
                    } else {
                        b::eq(l.scalar()?, r.scalar()?)
                    },
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
            J::Ident(n) if n == "this" && !self.methods[id].syntax.is_static() => {
                return Ok(JavaType::Class(owner.into()))
            }
            J::New { java_type, .. } => return Ok(java_type.clone()),
            J::MethodCall { target: Some(receiver), method, .. } => {
                if let Some(field) = crate::normalized::source_ports::receiver(receiver) {
                    if let Some(port) = self.project.source_ports.get(&format!("{owner}.{field}.{method}")) {
                        return Ok(port.returns.clone());
                    }
                }
            }
            J::FieldAccess { target, field } => {
                let JavaType::Class(owner) = self.source_declared_type(target, id)? else {
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
            }).or_else(|| {
                if method.is_static() || !self.is_entry_state(owner, name) { return None; }
                self.program.find_class(owner)?.fields().iter()
                    .find(|field| field.name() == name).map(|field| field.java_type().clone())
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
        if matches!(expression, J::Lit(JavaLit::Null)) {
            return Ok(());
        }
        let interface = self.program.find_class(owner).is_some_and(|class| crate::normalized::plain_source_interface(class));
        if interface {
            if let J::Ternary { then_expr, else_expr, .. } = expression {
                self.source_reference_assignment(expected, then_expr, id)?;
                return self.source_reference_assignment(expected, else_expr, id);
            }
        }
        // Narrowing Object to a source class is never inferred from a Heap
        // identity or a coincidental field name. This bounded entrance accepts
        // declared local/parameter identities of the same owner only.
        let actual = self.source_declared_type(expression, id)?;
        need(
            self.project.source_owner(&actual) == Some(owner) || interface
                && self.project.source_owner(&actual).and_then(|name| self.program.find_class(name))
                    .is_some_and(|class| class.interfaces().iter().any(|name| name == owner)),
            "source reference assignment requires the declared owner or interface type",
        )
    }
    fn source_field(&self, receiver: &J, field: &str, id: &str) -> Result<()> {
        let ty = self.source_declared_type(receiver, id)?;
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
        self.source_steps(body, id, env, path)?;
        let terminal = self.realtime_completion.terminal_alternatives();
        if let Some(result) = terminal.as_ref().and_then(|exits| exits.normal_return_value()) {
            return Ok(Value::Term(self.methods[id].returns.clone(),result));
        }
        if self.methods[id].syntax.is_constructor() { return Ok(Value::Term(Ty::Unit,b::lit_null())); }
        if terminal.as_ref().is_some_and(|exits| exits.is_always_throwing()) {
            // No expression can observe this carrier; the completion records
            // the actual exit and enclosing catches consume that completion.
            return Ok(Value::Term(self.methods[id].returns.clone(), b::lit_null()));
        }
        Err(fail("missing return"))
    }

    pub(super) fn source_steps(&mut self, body: &[S], id: &str, env: &mut Env, entry_path: Expr) -> Result<()> {
        let mut consumed_through = 0;
        for (i, s) in body.iter().enumerate() {
            if i < consumed_through { continue; }
            let path = math::normalize(b::and(entry_path.clone(), self.realtime_completion.continuing_guard()));
            if path.as_bool() == Some(false) { break; }
            self.budget()?;
            match s {
                S::Block(statements) => self.source_scope(statements, id, env, path)?,
                S::Return(Some(expr)) => {
                    need(
                        i + 1 == body.len(),
                        "unreachable supplied statements must be removed",
                    )?;
                    self.source_reference_assignment(self.methods[id].syntax.return_type(), expr, id)?;
                    let value = self.java_expr(expr, id, env, path)?;
                    let value = if crate::normalized::native_decimal_type(self.methods[id].syntax.return_type()) {
                        Self::promote_native_decimal(value)
                    } else { value };
                    let realtime_value = value.scalar()?;
                    self.realtime_completion = self
                        .realtime_completion
                        .replace_continue(realtime_state::Completion::Return(realtime_value));
                    return Ok(());
                }
                S::Throw(expr) => {
                    use crate::normalized::RuntimeThrowMessage;
                    let payload = match crate::normalized::runtime_throw_message(expr)
                        .ok_or_else(|| fail("source throw needs an explicit RuntimeException message contract"))?
                    {
                        RuntimeThrowMessage::Absent => None,
                        RuntimeThrowMessage::Null => Some(b::lit_null()),
                        RuntimeThrowMessage::Text(text) => Some(b::lit_str(text)),
                    };
                    self.realtime_throws.push(path);
                    self.realtime_completion = self.realtime_completion.replace_continue(
                        realtime_state::Completion::Throw {
                            class: crate::normalized::RUNTIME_EXCEPTION.into(),
                            payload,
                        },
                    );
                }
                S::VarDecl {
                    java_type,
                    name,
                    initializer: None,
                    ..
                } => {
                    let binds = match body.get(i+1) {
                        Some(S::LocalIr(normalizer_ir::local::LocalIr::LocalMapEmpty { binding })) =>
                            binding == name && super::source_map::declared_map(java_type),
                        Some(S::LocalIr(normalizer_ir::local::LocalIr::LocalMapGet { binding, .. })) =>
                            binding == name && super::source_map::declared_text(java_type),
                        Some(S::LocalIr(normalizer_ir::local::LocalIr::MemberChoice { binding, .. })) =>
                            binding == name && (self.project.parameter_type(java_type)? == Ty::Integer
                                || self.project.source_owner(java_type).is_some()),
                        Some(S::LocalIr(normalizer_ir::local::LocalIr::ProjectedUnionCount { binding, .. })) =>
                            binding == name && self.project.parameter_type(java_type)? == Ty::Integer,
                        Some(S::LocalIr(normalizer_ir::local::LocalIr::SqlWrite(w))) => &w.binding == name && self.project.parameter_type(java_type)?==Ty::Integer,
                        Some(S::LocalIr(normalizer_ir::local::LocalIr::CountRange { binding, accumulate: false, .. })) => binding == name && self.project.parameter_type(java_type)? == Ty::Integer,
                        Some(S::LocalIr(normalizer_ir::local::LocalIr::Relation(q))) => &q.binding==name && !q.accumulate,
                        Some(S::LocalIr(normalizer_ir::local::LocalIr::Decimal(decimal))) =>
                            &decimal.binding == name && crate::normalized::exact_decimal_type(java_type),
                        _=>false,
                    };
                    need(binds && !env.contains_key(name),"uninitialized local requires a following typed local IR definition")?;
                }
                S::LocalIr(normalizer_ir::local::LocalIr::Stack(_)) => {
                    consumed_through = i + self.execute_closed_source_stack(&body[i..], id, env, path)?;
                }
                S::LocalIr(normalizer_ir::local::LocalIr::LocalMapEmpty { binding }) => {
                    need(matches!(i.checked_sub(1).and_then(|i| body.get(i)),
                        Some(S::VarDecl { name, java_type, initializer: None, .. }) if name == binding
                            && super::source_map::declared_map(java_type)), "local map requires a fresh typed local declaration")?;
                    env.insert(binding.clone(), Value::LocalTextMap(Default::default()));
                }
                S::LocalIr(normalizer_ir::local::LocalIr::LocalMapPut { binding, key, value }) => {
                    let value = super::source_map::text_operand(value, env)?;
                    let Some(Value::LocalTextMap(map)) = env.get_mut(binding) else {
                        return Err(fail("local map update requires its fresh map binding"));
                    };
                    map.put(key.clone(), value);
                }
                S::LocalIr(normalizer_ir::local::LocalIr::LocalMapGet { binding, source, key, fallback }) => {
                    need(matches!(i.checked_sub(1).and_then(|i| body.get(i)),
                        Some(S::VarDecl { name, java_type, initializer: None, .. }) if name == binding
                            && super::source_map::declared_text(java_type)), "local map read requires an uninitialized text local")?;
                    let method = self.methods[id].syntax;
                    let owner = id.rsplit_once('.').and_then(|(owner, _)| self.program.find_class(owner));
                    need(owner.is_some_and(|owner| crate::normalized::nullable_text_return(owner, method)),
                        "local map text result must declare a nullable return")?;
                    let fallback = super::source_map::text_operand(fallback, env)?;
                    let Some(Value::LocalTextMap(map)) = env.get(source) else {
                        return Err(fail("local map read requires its fresh map binding"));
                    };
                    env.insert(binding.clone(), Value::Term(Ty::Text, map.get_or(key, fallback)));
                }
                S::LocalIr(normalizer_ir::local::LocalIr::ProjectedUnionCount { binding, branches }) => {
                    need(matches!(i.checked_sub(1).and_then(|i| body.get(i)),
                        Some(S::VarDecl { name, initializer: None, .. }) if name == binding),
                        "projected union count must initialize its declared local")?;
                    self.projected_union_count(binding, branches, env)?;
                }
                S::LocalIr(normalizer_ir::local::LocalIr::Relation(query))=>{
                    let decl=self.source_declared_type(&J::Ident(query.binding.clone()),id)?;
                    self.execute_query(query, Some(&decl), env, &path)?;
                }
                S::LocalIr(normalizer_ir::local::LocalIr::Decimal(decimal)) => {
                    let decl = self.source_declared_type(&J::Ident(decimal.binding.clone()), id)?;
                    self.execute_source_decimal(decimal, &decl, env)?;
                }
                S::LocalIr(normalizer_ir::local::LocalIr::SqlWrite(write)) => {
                    let declared = matches!(i.checked_sub(1).and_then(|previous| body.get(previous)),
                        Some(S::VarDecl { name, initializer: None, .. }) if name == &write.binding)
                        || env
                            .get(&write.binding)
                            .is_some_and(|value| value.ty() == Ty::Integer);
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
                    need(
                        !simulator_parser::is_reserved_identifier(name),
                        "reserved Java local name; rename during preprocessing",
                    )?;
                    need(!env.contains_key(name), "duplicate local")?;
                    self.source_reference_assignment(java_type, expr, id)?;
                    let v = self.java_expr(expr, id, env, path.clone())?;
                    let v = if crate::normalized::native_decimal_type(java_type) {
                        Self::promote_native_decimal(v)
                    } else { v };
                    v.check_java_array_component(primitive_array_component(java_type))?;
                    need(v.ty() != Ty::LocalRelation, "local relation requires a typed query, not a Java scalar binding")?;
                    need(
                        v.ty() == self.project.declared_result_type(java_type)?
                            || (matches!(v.ty(), Ty::Record(_)) && self.project.source_owner(java_type).is_some()
                                && self.source_declared_type(expr, id)? == *java_type),
                        "local type mismatch",
                    )?;
                    env.insert(name.clone(), v);
                }
                S::Assign { target: J::ArrayAccess { target, index }, value, .. } => {
                    let array = self.java_expr(target, id, env, path.clone())?;
                    let index = self.java_expr(index, id, env, path.clone())?;
                    // A simple store evaluates its RHS before its bounds check.
                    let rhs = self.java_expr(value, id, env, path.clone())?;
                    self.source_array_store(array, index, rhs, path)?;
                }
                S::Assign {target: J::Ident(name), value, ..} => {
                    if let Some(local) = env.get(name) {
                        let ty = local.ty();
                        need(ty != Ty::LocalTextMap, "local map binding cannot be reassigned as a Java alias")?;
                        let java_component = local.java_array_component().map(str::to_owned);
                        let value = self.java_expr(value,id,env,path)?;
                        let value = if ty == Ty::Decimal { Self::promote_native_decimal(value) } else { value };
                        value.check_java_array_component(java_component.as_deref())?;
                        need(value.ty() == ty,"local assignment type mismatch")?;
                        env.insert(name.clone(),value);
                    } else if !self.methods[id].syntax.is_static() && !env.contains_key("this")
                        && self.is_entry_state(id.rsplit_once('.').unwrap().0, name)
                    {
                        let value = self.java_expr(value,id,env,path.clone())?;
                        self.write_entry_state(id.rsplit_once('.').unwrap().0, name, value, path)?;
                    } else {
                        return Err(fail(format!("undeclared assignment target {name}")));
                    }
                }
                S::Assign {
                    target: J::FieldAccess { target, field },
                    value,
                    ..
                } => {
                    self.source_field(target, field, id)?;
                    if matches!(target.as_ref(), J::Ident(name) if name == "this") && !env.contains_key("this") {
                        let value = self.java_expr(value, id, env, path.clone())?;
                        self.write_entry_state(id.rsplit_once('.').unwrap().0, field, value, path)?;
                        continue;
                    }
                    let reference = self.java_expr(target, id, env, path.clone())?;
                    // Java evaluates RHS before the target putfield null check.
                    let value = self.java_expr(value, id, env, path.clone())?;
                    if self.source_objects.owns(&reference.scalar()?)
                    {
                        self.write_source_field(&reference.scalar()?, field, value, path.clone())?;
                    } else {
                        return Err(fail("field write requires a closed source object"));
                    }
                }
                S::If {
                    cond,
                    then_block,
                    else_block,
                } => {
                    let test=self.java_expr(cond,id,env,path.clone())?;
                    let condition=self.condition_under_path(test.scalar()?,&path);
                    self.source_if(condition,then_block,else_block.as_deref().unwrap_or_default(),id,env,path)?;
                }
                S::For {init,cond,update,body} => {
                    if let Some(rectangle) = crate::normalized::fixed_for::RectangularCounter::from_for(s) {
                        self.execute_rectangular_counter(rectangle, env)?;
                        continue;
                    }
                    let header = crate::normalized::fixed_for::FixedFor::from_header(init.as_deref(),cond.as_ref(),update.as_ref())
                        .ok_or_else(||fail("for loop needs a proven fixed induction bound"))?;
                    self.execute_fixed_for(header,body,id,env,path)?;
                }
                S::Try {body,catches,finally_block} => {
                    self.execute_source_try(body,catches,finally_block.as_deref(),id,env,path)?;
                }
                S::LocalIr(normalizer_ir::local::LocalIr::MemberChoice { binding, members, empty }) => {
                    let Some(S::VarDecl { name, java_type, initializer: None, .. }) = i.checked_sub(1).and_then(|i| body.get(i)) else {
                        return Err(fail("member choice must follow its uninitialized local declaration"));
                    };
                    need(name == binding, "member choice must follow its uninitialized local declaration")?;
                    self.choose_source_member(binding, members, empty, java_type, id, env, path)?;
                }
                S::LocalIr(normalizer_ir::local::LocalIr::FoldIndexRange { binding, lower, upper, skip_before, stop_after }) => {
                    self.execute_index_fold(binding, lower, upper, skip_before, stop_after, env)?;
                }
                S::LocalIr(normalizer_ir::local::LocalIr::CountRange {binding,accumulate,lower,upper}) => {
                    let declared = env.get(binding).is_some_and(|value| value.ty()==Ty::Integer)
                        || (!*accumulate && matches!(i.checked_sub(1).and_then(|previous| body.get(previous)),
                            Some(S::VarDecl { name, initializer:None,..}) if name == binding));
                    need(declared,"count requires a declared integer output")?;
                    self.count_range(binding,*accumulate,lower,upper,env)?;
                }
                S::ExprStmt(call @ J::MethodCall { .. }) => {
                    self.java_expr(call, id, env, path)?;
                }
                S::ForEach { var_type, var_name, iterable, body } if crate::normalized::integral(var_type) => {
                    let (total, offset) = crate::normalized::array_sum(var_name, body)
                        .ok_or_else(|| fail("for-each requires an exact source array sum"))?;
                    let array = self.java_expr(iterable, id, env, path.clone())?;
                    let prior = env.get(total).ok_or_else(|| fail("array sum needs an initialized accumulator"))?;
                    need(prior.ty() == Ty::Integer, "array sum needs a numeric accumulator")?;
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
            Value::Term(Ty::Integer, expr) | Value::IntegerFloor { value: expr, .. } => Value::Term(Ty::Decimal, expr),
            value => value,
        }
    }

    fn java_arithmetic(
        &mut self,
        op: JavaArithmetic,
        operands: (&J, &J),
        id: &str,
        env: &mut Env,
        path: Expr,
    ) -> Result<Value> {
        let (left, right) = operands;
        let left = self.java_expr(left, id, env, path.clone())?;
        let right = self.java_expr(right, id, env, path.clone())?;
        if op == JavaArithmetic::Add && (left.ty() == Ty::Text || right.ty() == Ty::Text) {
            let (left, right) = (left.scalar()?, right.scalar()?);
            let text = |value: Expr| match value.as_lit() {
                Some(math::Lit::Str(_)) => Some(value),
                Some(math::Lit::Null) => Some(b::lit_str("null")),
                _ => None,
            };
            if let (Some(left), Some(right)) = (text(left), text(right)) {
                return Ok(Value::Term(Ty::Text, math::normalize(b::add(left, right))));
            }
            return Err(fail("text concatenation requires concrete text or null operands"));
        }
        if left.ty() == Ty::Decimal || right.ty() == Ty::Decimal {
            need(matches!(left.ty(), Ty::Integer | Ty::Decimal)
                && matches!(right.ty(), Ty::Integer | Ty::Decimal), "numeric operands required")?;
            let binary = match op {
                JavaArithmetic::Add => BinOpKind::Add,
                JavaArithmetic::Sub => BinOpKind::Sub,
                JavaArithmetic::Mul => BinOpKind::Mul,
            };
            return Ok(Value::Term(Ty::Decimal,
                math::domains::decimal::binary(binary, left.scalar()?, right.scalar()?)));
        }
        need(
            left.ty() == Ty::Integer && right.ty() == Ty::Integer,
            "integer operands required",
        )?;
        if op == JavaArithmetic::Add
            && (matches!(left, Value::IntegerFloor { .. }) || matches!(right, Value::IntegerFloor { .. }))
        {
            let domain = self.normal_path(path.clone());
            if let (Some(left_floor), Some(right_floor)) =
                (left.certified_integer_floor(&domain), right.certified_integer_floor(&domain))
            {
                let value = math::normalize(b::add(left.scalar()?, right.scalar()?));
                return Ok(Value::IntegerFloor { value, lower: left_floor + right_floor, domain });
            }
        }
        let (left, right) = (left.scalar()?, right.scalar()?);
        // Operand evaluation is already recorded in S. An identical
        // numeric receipt can cancel only on a path that acquired it.
        if op == JavaArithmetic::Sub && left == right && self.acquired_integer_receipt(&left, &path) {
            return Ok(Value::Term(Ty::Integer, b::lit_int(0)));
        }
        Ok(Value::Term(Ty::Integer, math::normalize(op.build(left, right))))
    }

    fn evaluate_local_array_initializer(
        &mut self, element_type: &JavaType, elements: &[J], id: &str, env: &mut Env, path: Expr,
    ) -> Result<super::source_arrays::LocalIntArray> {
        need(crate::normalized::java_int(element_type), "unsupported Java expression in pure-array-1")?;
        let values = elements.iter().map(|element| self.java_expr(element, id, env, path.clone()))
            .collect::<Result<_>>()?;
        self.source_local_array_initializer(values)
    }

    fn evaluate_local_array_allocation(
        &mut self, element_type: &JavaType, length: &J, id: &str, env: &mut Env, path: Expr,
    ) -> Result<super::source_arrays::LocalIntArray> {
        need(crate::normalized::java_int(element_type), "unsupported Java expression in pure-array-1")?;
        let size = self.java_expr(length, id, env, path.clone())?;
        self.source_local_array_allocation(size, path)
    }

    fn local_java_array_length(&mut self, target: &J, id: &str, env: &mut Env, path: Expr) -> Result<Option<i64>> {
        match target {
            J::Ident(name) => {
                if let Some(Value::LocalIntArray(array)) = env.get(name) {
                    // Charge the child identifier visit just as generic expression
                    // evaluation did; the immutable length itself needs no path.
                    self.budget()?;
                    return Ok(Some(array.length()));
                }
                Ok(None)
            }
            J::ArrayInitializer { element_type, elements } => {
                self.budget()?;
                need(crate::normalized::java_int(element_type), "unsupported Java expression in pure-array-1")?;
                // The initializer still runs before .length. Its element
                // values are never observed, so no constant carrier is needed.
                for element in elements {
                    let active = self.normal_path(path.clone());
                    if active.as_bool() == Some(false) { break; }
                    let value = self.java_expr(element, id, env, active)?;
                    need(value.ty() == Ty::Integer, "expected Java int for local array")?;
                }
                Ok(Some(elements.len() as i64))
            }
            J::ArrayAllocation { element_type, length } => {
                self.budget()?;
                let path = self.normal_path(path);
                Ok(Some(self.evaluate_local_array_allocation(element_type, length, id, env, path)?.length()))
            }
            _ => Ok(None),
        }
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
                if let Some(value) = env.get(n) { return Ok(value.clone()); }
                let owner = id.rsplit_once('.').unwrap().0;
                if !self.methods[id].syntax.is_static() && !env.contains_key("this") && self.is_entry_state(owner, n) {
                    return self.read_entry_state(owner, n, &path);
                }
                Err(fail(format!("undefined Java variable {n}")))
            }
            J::Lit(JavaLit::Str(v)) => Ok(Value::Term(Ty::Text, b::lit_str(v))),
            // A char constant contributes its unsigned UTF-16 code unit to
            // a numeric expression, not a text value or reference identity.
            J::Lit(JavaLit::Char(v)) => Ok(Value::Term(Ty::Integer, b::lit_int(i64::from(*v)))),
            J::Lit(JavaLit::Null) => Ok(Value::Term(Ty::Ref, b::lit_null())),
            J::Lit(JavaLit::Bool(v)) => Ok(Value::Term(Ty::Bool, b::lit_bool(*v))),
            J::Lit(JavaLit::Decimal { digits, radix, power, .. }) => {
                let value = math::domains::decimal::from_radix_digits(digits, *radix, *power)
                    .expect("bounded numeric token is certified at source admission");
                Ok(Value::Term(Ty::Decimal, value))
            }
            J::Lit(JavaLit::Long(v)) => {
                Ok(Value::Term(Ty::Integer, b::lit_int(*v)))
            }
            J::Lit(JavaLit::Int(v)) => Ok(Value::Term(
                Ty::Integer,
                b::lit_int(i64::from(*v)),
            )),
            J::New {
                java_type: JavaType::Class(owner),
                args,
            } => {
                let args = args
                    .iter()
                    .map(|expr| self.java_expr(expr, id, env, path.clone()))
                    .collect::<Result<_>>()?;
                self.construct_source_object(owner, args, path)
            }
            J::ArrayInitializer { element_type, elements } => self
                .evaluate_local_array_initializer(element_type, elements, id, env, path)
                .map(Value::LocalIntArray),
            J::ArrayAllocation { element_type, length } => self
                .evaluate_local_array_allocation(element_type, length, id, env, path)
                .map(Value::LocalIntArray),
            J::ArrayAccess { target, index } => {
                let array = self.java_expr(target, id, env, path.clone())?;
                let index = self.java_expr(index, id, env, path.clone())?;
                self.source_array_read(array, index, path)
            }
            J::FieldAccess { target, field } => {
                if field == "length" {
                    if let Some(length) = self.local_java_array_length(target, id, env, path.clone())? {
                        return Ok(Value::Term(Ty::Integer, b::lit_int(length)));
                    }
                }
                self.source_field(target, field, id)?;
                if matches!(target.as_ref(), J::Ident(name) if name == "this")
                    && !env.contains_key("this")
                {
                    return self.read_entry_state(id.rsplit_once('.').unwrap().0, field, &path);
                }
                let reference = self.java_expr(target, id, env, path.clone())?;
                if matches!(reference.ty(), Ty::Record(_)) {
                    return values::field(reference, field);
                }
                if let Some(value)=self.read_selected_field(&reference.scalar()?,field,&path)? { return Ok(value); }
                if self.source_objects.owns(&reference.scalar()?) {
                    let reference = reference.scalar()?;
                    let null = self.source_object_null(&reference);
                    self.throw_source_on_path(b::and(path.clone(), null), "java.lang.NullPointerException");
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
                let owner = match target {
                    None => id.rsplit_once('.').unwrap().0,
                    Some(t) => crate::normalized::source_ports::receiver(t)
                        .ok_or_else(|| fail("Java method receiver must have a declared owner"))?,
                };
                if let Some(J::Ident(local)) = target.as_deref() {
                    if let Some(receiver) = env.get(local) {
                        if let JavaType::Class(interface) = self.source_declared_type(&J::Ident(local.clone()), id)? {
                            if self.program.find_class(&interface).is_some_and(crate::normalized::plain_source_interface) {
                                let receiver = receiver.scalar()?;
                                let values = args.iter().map(|arg| self.java_expr(arg, id, env, path.clone()))
                                    .collect::<Result<Vec<_>>>()?;
                                return self.invoke_interface(&interface, method, receiver, values, path);
                            }
                            // An ordinary input that shadows a marked receiver
                            // does not inherit the field's external contract.
                            if self.program.find_class(&interface).is_some_and(|class| {
                                class.is_interface()
                                    && crate::normalized::source_ports::declares_bodyless_member(
                                        class, method, args.len(),
                                    )
                            }) && receiver.scalar().is_ok_and(|reference| {
                                reference.as_heap_ref().is_some()
                                    && self.source_objects.allocation_owner(&reference).is_none()
                            }) {
                                crate::preprocess::out_of_source(
                                    self.selected_entry.unwrap_or(id),
                                    &format!("{interface}.{method}"),
                                );
                            }
                        }
                    }
                }
                let field_receiver = matches!(target.as_deref(), Some(J::FieldAccess { .. }));
                need(field_receiver || !env.contains_key(owner), "instance calls unsupported")?;
                let callee = format!("{owner}.{method}");
                let port = format!("{}.{}", id.rsplit_once('.').unwrap().0, callee);
                if self.project.source_ports.contains_key(&port) {
                    need(target.is_some() && !self.methods[id].syntax.is_static(), "external instance field requires a bound receiver")?;
                    let values = args.iter()
                        .map(|arg| self.java_expr(arg, id, env, path.clone()))
                        .collect::<Result<_>>()?;
                    return self.source_port_call(&port, values, path);
                }
                need(!field_receiver, "field receiver is not a declared external port")?;
                if let Some(declaration) = self.methods.get(&callee) {
                    for ((expected, _), actual) in declaration.syntax.params().iter().zip(args) {
                        self.source_reference_assignment(expected, actual, id)?;
                    }
                }
                let vs: Vec<_> = args
                    .iter()
                    .map(|a| self.java_expr(a, id, env, path.clone()))
                    .collect::<Result<_>>()?;
                if let Some(intrinsic) = self
                    .program
                    .find_class(id.rsplit_once('.').unwrap().0)
                    .and_then(|caller| {
                        crate::normalized_text::source_intrinsic(caller, &callee, vs.len())
                    })
                {
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
                        SourceIntrinsic::DefaultIfBlank => b::normal_return_case_when(vec![
                            (
                                crate::normalized_text::is_blank(value.clone()),
                                vs[1].scalar()?,
                            ),
                            (b::lit_bool(true), value),
                        ]),
                    };
                    return Ok(Value::Term(Ty::Text, math::normalize(expr)));
                }
                if target.is_none() && !self.methods[id].syntax.is_static() {
                    return self.java_call_with_receiver(&callee, vs, path, env.get("this").cloned(), true);
                }
                self.java_call(&callee, vs, path)
            }
            J::UnaryOp {
                op: U::Not,
                operand,
            } => {
                let v = self.java_expr(operand, id, env, path)?;
                need(v.ty() == Ty::Bool, "not requires bool")?;
                Ok(Value::Term(Ty::Bool, b::not(v.scalar()?)))
            }
            J::Cast {
                target_type,
                operand,
            } if matches!(target_type, JavaType::Primitive(n) if n == "long") =>
            {
                let operand = self.java_expr(operand, id, env, path)?;
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
                let value = self.java_expr(operand, id, env, path)?;
                if value.ty() == Ty::Decimal {
                    return Ok(Value::Term(Ty::Decimal, math::domains::decimal::negate(value.scalar()?)));
                }
                need(value.ty() == Ty::Integer, "negation requires an integer")?;
                Ok(Value::Term(
                    Ty::Integer,
                    math::normalize(b::neg(value.scalar()?)),
                ))
            }
            J::BinOp { op: B::Add, left, right } =>
                self.java_arithmetic(JavaArithmetic::Add, (left, right), id, env, path),
            J::BinOp { op: B::Sub, left, right } =>
                self.java_arithmetic(JavaArithmetic::Sub, (left, right), id, env, path),
            J::BinOp { op: B::Mul, left, right } =>
                self.java_arithmetic(JavaArithmetic::Mul, (left, right), id, env, path),
            J::BinOp { op: op @ (B::Div | B::Mod), left, right } => {
                let left = self.java_expr(left, id, env, path.clone())?;
                let normal = self.normal_path(path.clone());
                let no_value = || Value::Term(Ty::Integer, b::lit_null());
                if normal.as_bool() == Some(false) {
                    return Ok(no_value());
                }
                let right = self.java_expr(right, id, env, normal)?;
                let normal = self.normal_path(path);
                if normal.as_bool() == Some(false) {
                    return Ok(no_value());
                }
                need(left.ty() == Ty::Integer && right.ty() == Ty::Integer,
                    "division and remainder require integer operands")?;
                let concrete = |v: Value| v.scalar()?.as_lit().and_then(math::Lit::as_integer)
                    .ok_or_else(|| crate::preprocess::required(
                        "NORM-EXPRESSION", id, "division and remainder require concrete integers",
                        "保留原操作数和异常行为；当前入口不接纳符号化商或余数。",
                    ));
                let (left, right) = (concrete(left)?, concrete(right)?);
                if right == math::Integer::from(0) {
                    self.throw_source_on_path(normal, "java.lang.ArithmeticException");
                    return Ok(no_value());
                }
                let result = if *op == B::Div { left / right } else { left % right };
                Ok(Value::Term(Ty::Integer, b::lit_integer(result)))
            }
            J::BinOp { op, left, right } if matches!(op, B::And | B::Or) => {
                let left = self.java_expr(left, id, env, path.clone())?;
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
                if math::normalize(b::and(path.clone(), on_right.clone())).as_bool() == Some(false) {
                    return Ok(Value::Term(Ty::Bool, b::lit_bool(*op == B::Or)));
                }
                let right = self.java_expr(right, id, env, b::and(path, on_right))?;
                need(
                    right.ty() == Ty::Bool,
                    "Java short-circuit right operand must be boolean",
                )?;
                if *op == B::And {
                    self.choose(
                        condition,
                        right,
                        Value::Term(Ty::Bool, b::lit_bool(false)),
                    )
                } else {
                    self.choose(
                        condition,
                        Value::Term(Ty::Bool, b::lit_bool(true)),
                        right,
                    )
                }
            }
            J::Ternary {
                cond,
                then_expr,
                else_expr,
            } => {
                let condition = self.java_expr(cond, id, env, path.clone())?;
                let condition = self.condition_under_path(condition.scalar()?, &path);
                if let Some(taken) = condition.as_bool() {
                    return self.java_expr(if taken { then_expr } else { else_expr }, id, env, path);
                }
                let yes =
                    self.java_expr(then_expr, id, env, b::and(path.clone(), condition.clone()))?;
                let no =
                    self.java_expr(else_expr, id, env, b::and(path, b::not(condition.clone())))?;
                let (yes, no) = if matches!((yes.ty(), no.ty()), (Ty::Integer, Ty::Decimal) | (Ty::Decimal, Ty::Integer)) {
                    (Self::promote_native_decimal(yes), Self::promote_native_decimal(no))
                } else { (yes, no) };
                self.choose(condition, yes, no)
            }
            J::BinOp { op, left, right } => {
                let op = match op {
                    B::Eq => BinOpKind::Eq,
                    B::Ne => BinOpKind::Ne,
                    B::Lt => BinOpKind::Lt,
                    B::Le => BinOpKind::Le,
                    B::Gt => BinOpKind::Gt,
                    B::Ge => BinOpKind::Ge,
                    _ => return Err(fail("unsupported Java expression in pure-array-1")),
                };
                let nullable = match (left.as_ref(), right.as_ref()) {
                    (value, J::Lit(JavaLit::Null)) | (J::Lit(JavaLit::Null), value)
                        if matches!(op, BinOpKind::Eq | BinOpKind::Ne) => Some(value),
                    _ => None,
                };
                if let Some(operand) = nullable {
                    let v = self.java_expr(operand, id, env, path)?;
                    let presence = match &v { Value::Term(Ty::Ref,r)=>self.row_presence(r),_=>None };
                    let null = if let Some(present)=presence { b::not(present) } else if matches!(&v, Value::Term(Ty::Ref, reference)
                        if self.source_objects.owns(reference) || self.source_objects.has_complete_identity_partition(reference))
                    {
                        self.source_reference_equality(v.scalar()?, b::lit_null(), false)?
                    } else {
                        match v {
                            Value::Sequence { null, .. } => null,
                            Value::Term(Ty::Ref | Ty::Text, e) => b::eq(e, b::lit_null()),
                            _ => return Err(fail("null comparison type")),
                        }
                    };
                    return Ok(Value::Term(
                        Ty::Bool,
                        if op == BinOpKind::Ne { b::not(null) } else { null },
                    ));
                }
                let l = self.java_expr(left, id, env, path.clone())?;
                need(
                    l.ty() != Ty::Text,
                    "Java text identity comparison requires explicit identity representation",
                )?;
                let r = self.java_expr(right, id, env, path.clone())?;
                if matches!((l.ty(), r.ty()), (Ty::Decimal, Ty::Decimal) | (Ty::Decimal, Ty::Integer) | (Ty::Integer, Ty::Decimal)) {
                    return Ok(Value::Term(Ty::Bool, math::domains::decimal::binary(op, l.scalar()?, r.scalar()?)));
                }
                need(l.ty() == r.ty(), "Java comparison type mismatch")?;
                let ty = l.ty();
                need(
                    matches!(op, BinOpKind::Eq | BinOpKind::Ne) || matches!(ty, Ty::Integer),
                    "ordered comparison type",
                )?;
                if matches!(l, Value::IntegerFloor { .. }) || matches!(r, Value::IntegerFloor { .. }) {
                    let domain = self.normal_path(path);
                    if let Some(value) = compare_integer_floor(op, &l, &r, &domain) {
                        return Ok(Value::Term(Ty::Bool, b::lit_bool(value)));
                    }
                }
                let a = l.scalar()?;
                let z = r.scalar()?;
                need(!matches!(ty, Ty::Record(_)),
                    "member choice record has content but no Java reference identity")?;
                if ty == Ty::Ref {
                    let identity = self.source_reference_equality(a, z, op == BinOpKind::Ne)?;
                    return Ok(Value::Term(Ty::Bool, identity));
                }
                Ok(Value::Term(
                    Ty::Bool,
                    b::binop_public(op, a, z),
                ))
            }
            _ => Err(fail("unsupported Java expression in pure-array-1")),
        }
    }
}

// Apply the source host's proof before entering the unconstrained math cache.
fn compare_integer_floor(op: BinOpKind, left: &Value, right: &Value, domain: &Expr) -> Option<bool> {
    fn against_literal(op: BinOpKind, subject: &Value, bound: &Value, domain: &Expr) -> Option<bool> {
        let lower = subject.certified_integer_floor(domain)?;
        let Value::Term(Ty::Integer, bound) = bound else { return None; };
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
    pub(super) fn emit_stage_profile(
        &self,
        stage: &str,
        started: Instant,
        status: &str,
    ) {
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
            crate::profile::event(
                "kernel_execution.metrics",
                &self.profile_metrics(status),
            );
        }
    }
    pub(super) fn condition_under_path(&self, condition: Expr, path: &Expr) -> Expr {
        let condition = math::normalize(condition);
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
        let Some(SymVarOrigin::External { binding_id, path: fields, primitive_type }) = value.as_sym_var() else {
            return false;
        };
        if primitive_type != "integer" || !fields.is_empty() { return false; }
        self.realtime_effects.events().iter().any(|event| {
            let realtime_state::EffectOperation::Port { id, receipt } = &event.operation else { return false; };
            receipt.binding_id() == *binding_id
                && self.project.source_ports.get(id).is_some_and(|port| port.throws.is_empty())
                && b::path_implies(&self.normal_path(path.clone()), &event.guard)
        })
    }

    pub(super) fn throw_source_on_path(&mut self, path: Expr, exception: &str) {
        let path = math::normalize(self.normal_path(path));
        if path.as_bool() != Some(false) {
            self.realtime_completion = self.realtime_completion.replace_continue_with_throw(
                path.clone(), exception, None,
            );
            self.realtime_throws.push(path);
        }
    }

    pub(super) fn acquire_source_state_root(&mut self, key: String) -> Expr {
        let id = *self.state_roots.entry(key).or_insert_with(|| self.heap.new_path_object().id());
        b::heap_ref(id)
    }

    pub(super) fn normal_path(&self, path: Expr) -> Expr {
        self.realtime_throws.iter().fold(path, |p, thrown| {
            math::normalize(b::and(p, b::not(thrown.clone())))
        })
    }
    pub(super) fn budget(&mut self) -> Result<()> {
        self.steps += 1;
        if self.steps < 1_000_000 && self.active.len() < 128 {
            Ok(())
        } else {
            if !self.budget_reported {
                self.budget_reported = true;
                self.emit_profile_metrics("budget_exceeded");
            }
            Err(fail(format!(
                "definition expansion budget exceeded: steps={} limit=1000000; active_calls={} limit=128",
                self.steps, self.active.len()
            )))
        }
    }

    fn invoke_interface(
        &mut self, interface: &str, member: &str, receiver: Expr, args: Vec<Value>, path: Expr,
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
                if branch.as_bool() == Some(false) { continue; }
                let effects = self.realtime_effects.events().len();
                let throws = self.realtime_throws.len();
                let completion = self.realtime_completion.clone();
                let value = self.invoke_interface(interface, member, selected.clone(), args.clone(), branch)?;
                need(effects == self.realtime_effects.events().len() && throws == self.realtime_throws.len()
                    && completion == self.realtime_completion, "conditional interface dispatch requires total overrides without external effects")?;
                need(ty.as_ref().is_none_or(|t| *t == value.ty()), "interface return types differ")?;
                ty = Some(value.ty());
                values.push((guard, value.scalar()?));
            }
            need(remaining.as_bool() == Some(false), "interface receiver alternatives do not cover the source condition")?;
            let ty = ty.ok_or_else(|| fail("no live interface receiver"))?;
            values.last_mut().unwrap().0 = b::lit_bool(true);
            return Ok(Value::Term(ty, math::normalize(b::case_when(values))));
        }
        need(!receiver.is_null(), "null interface receiver requires a closed null-completion contract")?;
        let target = format!("{interface}.{member}");
        let owner = self.source_objects.allocation_owner(&receiver)
            .unwrap_or_else(|| crate::preprocess::out_of_source(self.selected_entry.unwrap_or("Java execution"), &target));
        let class = self.program.find_class(owner).ok_or_else(|| fail("missing selected implementation"))?;
        need(class.interfaces().iter().any(|name| name == interface), "selected receiver does not implement declared interface")?;
        let actual = format!("{owner}.{member}");
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

    pub(super) fn method_parameter_type(&self, id: &str, index: usize, name: &str) -> Result<Ty> {
        let method = &self.methods[id];
        if let Some(binding) = &method.binding {
            let semantic = &binding.function.params[index];
            if semantic.name == name {
                return Ok(binding.parameters[index].clone());
            }
            // A manually prepared carrier can bind a different semantic name.
            // Resolve that rare Java-side override at the original call site.
            if let Some(ty) = self.project.parameter_override(id.rsplit('.').next().unwrap_or(id), name)? {
                return Ok(ty);
            }
        }
        Ok(method.defaults[index].clone())
    }

    fn java_call_with_receiver(
        &mut self, id: &str, mut args: Vec<Value>, path: Expr,
        receiver: Option<Value>, implicit_receiver: bool,
    ) -> Result<Value> {
        self.budget()?;
        self.java_calls += 1;
        let nested_call = !self.active.is_empty();
        let parent_completion = self.realtime_completion.clone();
        need(
            self.active.insert(format!("java:{id}")),
            "recursive Java call",
        )?;
        let m = self.methods.get(id)
            .unwrap_or_else(|| crate::preprocess::out_of_source(self.selected_entry.unwrap_or("Java execution"), id))
            .syntax;
        need(m.params().len() == args.len(), "Java arity mismatch")?;
        for (index, ((t, name), v)) in m.params().iter().zip(&mut args).enumerate() {
            let expected = self.method_parameter_type(id, index, name)?;
            if crate::normalized::native_decimal_type(t) {
                if v.ty() == Ty::Integer {
                    *v = Self::promote_native_decimal(v.clone());
                }
            }
            if expected == Ty::Text
                && matches!(v, Value::Term(Ty::Ref, e) if e.as_lit() == Some(&math::Lit::Null))
            {
                *v = Value::Term(Ty::Text, b::lit_null());
            }
            need(expected == v.ty(), "Java argument type mismatch")?;
        }
        let value = if let Some(binding) = self.methods[id].binding.clone() {
            self.call(id, &binding, args, path)?
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
            let body = m.body().unwrap_or_else(|| crate::preprocess::out_of_source(self.selected_entry.unwrap_or("Java execution"), id));
            self.block(body, id, &mut env, path)?
        };
        need(
            value.ty() == self.methods[id].returns,
            "Java return type mismatch",
        )?;
        let callee_completion = self.realtime_completion.clone();
        self.realtime_completion = if nested_call {
            parent_completion.replace_continue(callee_completion.resume_after_normal_return())
        } else {
            callee_completion
        };
        self.active.remove(&format!("java:{id}"));
        Ok(value)
    }
    fn call(&mut self, id: &str, binding: &CheckedBinding<'_>, args: Vec<Value>, path: Expr) -> Result<Value> {
        self.budget()?;
        self.semantic_calls += 1;
        let name = binding.name;
        need(
            self.active.insert(format!("semantic:{name}")),
            "recursive semantic call",
        )?;
        let f = binding.function;
        need(f.params.len() == args.len(), "semantic arity mismatch")?;
        for (ty, v) in binding.parameters.iter().zip(&args) {
            need(*ty == v.ty(), "semantic argument type mismatch")?;
        }
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
        let expected = &self.methods[id].returns;
        let actual = value.ty();
        if actual != *expected {
            return Err(fail(format!(
                "semantic result type mismatch: expected={} actual={}",
                expected.describe(),
                actual.describe()
            )));
        }
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
