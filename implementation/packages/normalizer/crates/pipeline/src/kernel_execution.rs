use super::*;

pub(super) struct Engine<'a> {
    pub(super) program: &'a JavaProgram,
    pub(super) source_relations:super::business_relation::Relations,
    pub(super) source_objects: super::source_objects::SourceObjects,
    pub(super) source_array_writes: BTreeMap<String, BTreeSet<i64>>,
    // These are deliberately separate from the work budget.  A run can do
    // relatively few logical steps while still building a native Rust call
    // chain deep enough to abort the process.  Keep the first guard
    // conservative until the evaluator is moved to an explicit worklist.
    pub(super) semantic_depth: usize,
    pub(super) node_depth: usize,
    pub(super) max_semantic_depth: usize,
    pub(super) max_node_depth: usize,
    pub(super) project: &'a Project,
    pub(super) methods: BTreeMap<String, &'a JavaMethod>,
    pub(super) bindings: BTreeMap<String, String>,
    pub(super) active: BTreeSet<String>,
    pub(super) steps: usize,
    pub(super) async_depth: usize,
    pub(super) settled_task_frame: bool,
    pub(super) selected_entry: Option<&'a str>,
    pub(super) entry_context: Option<&'a business_entry::EntryContext>,
    pub(super) state_roots: BTreeMap<String, Expr>,
    pub(super) host_reads: BTreeMap<String, u64>,
    pub(super) suspension: u64,
    pub(super) pending_ports: BTreeMap<u64, port_completion::PortSpec>,
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

const MAX_SAFE_SEMANTIC_DEPTH: usize = 48;
const MAX_SAFE_NODE_DEPTH: usize = 128;



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
            Node::Field { value, name } => values::field(self.node(value, env, path)?, name),
            Node::Settled { returns, steps } => {
                tasks::validate_settled_scope(steps)?;
                self.action_scope(returns, steps, env, path, true)
            }
            Node::Action { returns, steps } => {
                let parent_completion = self.realtime_completion.clone();
                let value = self.action_scope(returns, steps, env, path, false)?;
                // A nested action that completes normally is a value-producing
                // call inside the caller. Its function-level completion has
                // already been substituted into the returned binding and must
                // not terminate the caller's continuation. Exceptional or
                // mixed completion remains attached for caller-side catch
                // dispatch.
                if self.realtime_completion.return_value().is_some() {
                    self.realtime_completion = parent_completion;
                }
                Ok(value)
            }
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
                    (Ty::Unit, serde_yaml::Value::Null) => b::lit_null(),
                    (Ty::Bool, serde_yaml::Value::Bool(v)) => b::lit_bool(*v),
                    (Ty::Integer | Ty::I32 | Ty::I64, serde_yaml::Value::Number(v)) => {
                        let x = v.as_i64().ok_or_else(|| fail("integer range"))?;
                        need(t != Ty::I32 || i32::try_from(x).is_ok(), "i32 range")?;
                        b::lit_int(x)
                    }
                    (Ty::Ref | Ty::Text, serde_yaml::Value::Null) => b::lit_null(),
                    (Ty::Vector(item), serde_yaml::Value::Null) if **item == Ty::I32 => {
                        b::lit_null()
                    }
                    (Ty::Text, serde_yaml::Value::String(v)) => b::lit_str(v),
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
                    matches!(l.ty(), Ty::Bool | Ty::Integer | Ty::I32 | Ty::I64 | Ty::Ref),
                    "composite equality is not declared",
                )?;
                let gt = matches!(n, Node::Gt { .. });
                need(
                    !gt || matches!(l.ty(), Ty::Integer | Ty::I32 | Ty::I64),
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
            J::Ident(n) if n == "this" && !self.methods[id].is_static() => {
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
        let method = self.methods[id];
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
        let value = self.source_steps(body, id, env, path)?;
        if let Some(result) = self.realtime_completion.normal_return_value() {
            return Ok(Value::Term(self.project.result_type(self.methods[id].return_type())?,result));
        }
        if self.methods[id].is_constructor() { return Ok(Value::Term(Ty::Unit,b::lit_null())); }
        value.ok_or_else(||fail("missing return"))
    }

    pub(super) fn source_steps(&mut self, body: &[S], id: &str, env: &mut Env, entry_path: Expr) -> Result<Option<Value>> {
        for (i, s) in body.iter().enumerate() {
            let path = math::normalize(b::and(entry_path.clone(), self.realtime_completion.continuing_guard()));
            if path.as_bool() == Some(false) { break; }
            self.budget()?;
            match s {
                S::Return(Some(expr)) => {
                    need(
                        i + 1 == body.len(),
                        "unreachable supplied statements must be removed",
                    )?;
                    self.source_reference_assignment(self.methods[id].return_type(), expr, id)?;
                    let value = self.java_expr(expr, id, env, path)?;
                    let realtime_value = value.scalar()?;
                    self.realtime_completion = self
                        .realtime_completion
                        .replace_continue(realtime_state::Completion::Return(realtime_value));
                    return Ok(Some(value));
                }
                S::VarDecl {
                    java_type,
                    name,
                    initializer: None,
                } => {
                    let binds = match body.get(i+1) {
                        Some(S::LocalIr(normalizer_ir::local::LocalIr::SqlWrite(w))) => &w.binding == name && self.project.parameter_type(java_type)?==Ty::Integer,
                        Some(S::LocalIr(normalizer_ir::local::LocalIr::CountRange { binding, accumulate: false, .. })) => binding == name && self.project.parameter_type(java_type)? == Ty::Integer,
                        Some(S::LocalIr(normalizer_ir::local::LocalIr::Relation(q))) => &q.binding==name && !q.accumulate,
                        Some(S::LocalIr(normalizer_ir::local::LocalIr::Decimal(decimal))) =>
                            &decimal.binding == name && crate::normalized::exact_decimal_type(java_type),
                        _=>false,
                    };
                    need(binds && !env.contains_key(name),"uninitialized local requires a following typed local IR definition")?;
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
                        java_name_not_reserved(name),
                        "reserved Java local name; rename during preprocessing",
                    )?;
                    need(!env.contains_key(name), "duplicate local")?;
                    self.source_reference_assignment(java_type, expr, id)?;
                    let v = self.java_expr(expr, id, env, path.clone())?;
                    need(
                        v.ty() == self.project.result_type(java_type)?
                            || (matches!(v.ty(), Ty::Record(_)) && self.project.source_owner(java_type).is_some()
                                && self.source_declared_type(expr, id)? == *java_type),
                        "local type mismatch",
                    )?;
                    env.insert(name.clone(), v);
                }
                S::Assign { target: J::ArrayAccess { target, index }, value } => {
                    let array = self.java_expr(target, id, env, path.clone())?;
                    let index = self.java_expr(index, id, env, path.clone())?;
                    // A simple store evaluates its RHS before its bounds check.
                    let rhs = self.java_expr(value, id, env, path.clone())?;
                    self.source_array_store(array, index, rhs, path)?;
                }
                S::Assign {target: J::Ident(name), value} => {
                    if let Some(local) = env.get(name) {
                        let ty = local.ty();
                        let value = self.java_expr(value,id,env,path)?;
                        need(value.ty() == ty,"local assignment type mismatch")?;
                        env.insert(name.clone(),value);
                    } else if !self.methods[id].is_static() && !env.contains_key("this")
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
                    need(test.ty()==Ty::Bool,"Java if requires boolean")?;
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
                S::ForEach { .. } | S::While { .. } => {
                    return Err(fail("raw loop: provide a semantic definition"))
                }
                _ => return Err(fail("unsupported Java statement in pure-array-1")),
            }
        }
        Ok(None)
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
                if !self.methods[id].is_static() && !env.contains_key("this") && self.is_entry_state(owner, n) {
                    return self.read_entry_state(owner, n, &path);
                }
                Err(fail(format!("undefined Java variable {n}")))
            }
            J::Lit(JavaLit::Str(v)) => Ok(Value::Term(Ty::Text, b::lit_str(v))),
            // A char constant contributes its unsigned UTF-16 code unit to
            // a numeric expression, not a text value or reference identity.
            J::Lit(JavaLit::Char(v)) => Ok(Value::Term(Ty::Integer, b::lit_int(i64::from(u32::from(*v))))),
            J::Lit(JavaLit::Null) => Ok(Value::Term(Ty::Ref, b::lit_null())),
            J::Lit(JavaLit::Bool(v)) => Ok(Value::Term(Ty::Bool, b::lit_bool(*v))),
            J::Lit(JavaLit::Long(v)) => {
                Ok(Value::Term(Ty::Integer, b::lit_int(*v)))
            }
            J::Lit(JavaLit::Int(v)) => Ok(Value::Term(
                Ty::Integer,
                b::lit_int(*v),
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
            J::ArrayAccess { target, index } => {
                let array = self.java_expr(target, id, env, path.clone())?;
                let index = self.java_expr(index, id, env, path.clone())?;
                self.source_array_read(array, index, path)
            }
            J::FieldAccess { target, field } => {
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
                    self.read_or_bind_source_field(&reference.scalar()?, field, &path)
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
                        }
                    }
                }
                let field_receiver = matches!(target.as_deref(), Some(J::FieldAccess { .. }));
                need(field_receiver || !env.contains_key(owner), "instance calls unsupported")?;
                let callee = format!("{owner}.{method}");
                let port = format!("{}.{}", id.rsplit_once('.').unwrap().0, callee);
                if self.project.source_ports.contains_key(&port) {
                    need(target.is_some() && !self.methods[id].is_static(), "external instance field requires a bound receiver")?;
                    let values = args.iter()
                        .map(|arg| self.java_expr(arg, id, env, path.clone()))
                        .collect::<Result<_>>()?;
                    return self.source_port_call(&port, values, path);
                }
                need(!field_receiver, "field receiver is not a declared external port")?;
                if let Some(declaration) = self.methods.get(&callee) {
                    for ((expected, _), actual) in declaration.params().iter().zip(args) {
                        self.source_reference_assignment(expected, actual, id)?;
                    }
                }
                let vs: Vec<_> = args
                    .iter()
                    .map(|a| self.java_expr(a, id, env, path.clone()))
                    .collect::<Result<_>>()?;
                if self
                    .program
                    .find_class(id.rsplit_once('.').unwrap().0)
                    .is_some_and(|caller| {
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
                    let expr = match method.as_str() {
                        "trim" => crate::normalized_text::trim(value),
                        "defaultIfBlank" => b::normal_return_case_when(vec![
                            (
                                crate::normalized_text::is_blank(value.clone()),
                                vs[1].scalar()?,
                            ),
                            (b::lit_bool(true), value),
                        ]),
                        _ => unreachable!(),
                    };
                    return Ok(Value::Term(Ty::Text, math::normalize(expr)));
                }
                if target.is_none() && !self.methods[id].is_static() {
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
                need(value.ty() == Ty::Integer, "negation requires an integer")?;
                Ok(Value::Term(
                    Ty::Integer,
                    math::normalize(b::neg(value.scalar()?)),
                ))
            }
            J::BinOp {
                op: op @ (B::Add | B::Sub | B::Mul),
                left,
                right,
            } => {
                let left = self.java_expr(left, id, env, path.clone())?;
                let right = self.java_expr(right, id, env, path.clone())?;
                if *op == B::Add && (left.ty() == Ty::Text || right.ty() == Ty::Text) {
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
                need(
                    left.ty() == Ty::Integer && right.ty() == Ty::Integer,
                    "integer operands required",
                )?;
                let (left, right) = (left.scalar()?, right.scalar()?);
                // Operand evaluation is already recorded in S. An identical
                // numeric receipt can cancel only on a path that acquired it.
                if *op == B::Sub && left == right && self.acquired_integer_receipt(&left, &path) {
                    return Ok(Value::Term(Ty::Integer, b::lit_int(0)));
                }
                let build = match op {
                    B::Add => b::add,
                    B::Sub => b::sub,
                    B::Mul => b::mul,
                    _ => unreachable!(),
                };
                Ok(Value::Term(
                    Ty::Integer,
                    math::normalize(build(left, right)),
                ))
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
                need(
                    condition.ty() == Ty::Bool,
                    "Java ternary condition must be boolean",
                )?;
                let condition = self.condition_under_path(condition.scalar()?, &path);
                if let Some(taken) = condition.as_bool() {
                    return self.java_expr(if taken { then_expr } else { else_expr }, id, env, path);
                }
                let yes =
                    self.java_expr(then_expr, id, env, b::and(path.clone(), condition.clone()))?;
                let no =
                    self.java_expr(else_expr, id, env, b::and(path, b::not(condition.clone())))?;
                self.choose(condition, yes, no)
            }
            J::BinOp { op, left, right }
                if matches!(op, B::Eq | B::Ne | B::Gt | B::Lt | B::Ge | B::Le) =>
            {
                let nullable = match (left.as_ref(), right.as_ref()) {
                    (value, J::Lit(JavaLit::Null)) | (J::Lit(JavaLit::Null), value)
                        if matches!(op, B::Eq | B::Ne) => Some(value),
                    _ => None,
                };
                if let Some(operand) = nullable {
                    let v = self.java_expr(operand, id, env, path)?;
                    let presence = match &v { Value::Term(Ty::Ref,r)=>self.row_presence(r),_=>None };
                    let null = if let Some(present)=presence { b::not(present) } else if matches!(&v, Value::Term(Ty::Ref, reference) if self.source_objects.owns(reference))
                    {
                        b::lit_bool(false)
                    } else {
                        match v {
                            Value::Sequence { null, .. } => null,
                            Value::Term(Ty::Ref | Ty::Text, e) => b::eq(e, b::lit_null()),
                            _ => return Err(fail("null comparison type")),
                        }
                    };
                    return Ok(Value::Term(
                        Ty::Bool,
                        if *op == B::Ne { b::not(null) } else { null },
                    ));
                }
                let l = self.java_expr(left, id, env, path.clone())?;
                need(
                    l.ty() != Ty::Text,
                    "Java text identity comparison requires explicit identity representation",
                )?;
                let r = self.java_expr(right, id, env, path)?;
                need(l.ty() == r.ty(), "Java comparison type mismatch")?;
                let ty = l.ty();
                need(
                    matches!(op, B::Eq | B::Ne) || matches!(ty, Ty::Integer | Ty::I32 | Ty::I64),
                    "ordered comparison type",
                )?;
                let mut a = l.scalar()?;
                let mut z = r.scalar()?;
                if ty == Ty::Ref {
                    self.check_borrowed_identity(&a, &z)?;
                    // Both operands have already been obtained in source order.
                    // Reorient only the pure comparison, never the evaluations.
                    let kind = if *op == B::Eq { math::BinOpKind::Eq } else { math::BinOpKind::Ne };
                    let names = crate::canonical_names::CanonicalHeapNames(
                        &self.source_objects.compared_parameters,
                    );
                    if math::canonical_order_ordered_comparison(kind, &a, &z, &names).is_some() {
                        std::mem::swap(&mut a, &mut z);
                    }
                }
                Ok(Value::Term(
                    Ty::Bool,
                    match op {
                        B::Eq => b::eq(a, z),
                        B::Ne => b::ne(a, z),
                        B::Gt => b::gt(a, z),
                        B::Lt => b::lt(a, z),
                        B::Ge => b::ge(a, z),
                        _ => b::le(a, z),
                    },
                ))
            }
            _ => Err(fail("unsupported Java expression in pure-array-1")),
        }
    }
}

impl Engine<'_> {
    pub(super) fn has_observable_boundary(&self) -> bool {
        !self.project.heap_fields.is_empty()
            || !self.project.ports.is_empty()
            || !self.project.exceptions.is_empty()
    }

    pub(super) fn profile_metrics(&self, status: &str) -> String {
        format!(
            "phase=kernel_execution status={status} steps={} semantic_calls={} java_calls={} node_visits={} branch_splits={} branch_collapses={} max_active_calls={} active_calls={} observable_boundary={} semantic_depth={} max_semantic_depth={} node_depth={} max_node_depth={} depth_limits=semantic:{MAX_SAFE_SEMANTIC_DEPTH},node:{MAX_SAFE_NODE_DEPTH}",
            self.steps,
            self.semantic_calls,
            self.java_calls,
            self.node_visits,
            self.branch_splits,
            self.branch_collapses,
            self.max_active_calls,
            self.active.len(),
            self.has_observable_boundary(),
            self.semantic_depth,
            self.max_semantic_depth,
            self.node_depth,
            self.max_node_depth,
        )
    }
    pub(super) fn emit_stage_profile(
        &self,
        stage: &str,
        started: Instant,
        status: &str,
        detail: impl AsRef<str>,
    ) {
        if crate::profile::enabled() {
            let detail = detail.as_ref();
            let detail_suffix = if detail.is_empty() {
                String::new()
            } else {
                format!(" {detail}")
            };
            crate::profile::event(
                "kernel_execution.stage",
                &format!(
                    "stage={stage} elapsed_us={} {}{}",
                    started.elapsed().as_micros(),
                    self.profile_metrics(status),
                    detail_suffix
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
        self.realtime_effects.events().iter().any(|event| {
            let realtime_state::EffectOperation::Port(id) = &event.operation else { return false; };
            event.result.as_ref() == Some(value)
                && self.project.source_ports.get(id).is_some_and(|port| port.throws.is_empty())
                && b::path_implies(&self.normal_path(path.clone()), &event.guard)
        })
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

    pub(super) fn depth_failure(&mut self, kind: &str, depth: usize, limit: usize) -> String {
        if !self.budget_reported {
            self.budget_reported = true;
            self.emit_profile_metrics("depth_exceeded");
        }
        fail(format!(
            "normalized depth budget exceeded: depth_kind={kind}; depth={depth}; limit={limit}; result=not-produced"
        ))
    }

    pub(super) fn enter_semantic_depth(&mut self) -> Result<()> {
        if self.semantic_depth >= MAX_SAFE_SEMANTIC_DEPTH {
            return Err(self.depth_failure(
                "semantic_call",
                self.semantic_depth + 1,
                MAX_SAFE_SEMANTIC_DEPTH,
            ));
        }
        self.semantic_depth += 1;
        self.max_semantic_depth = self.max_semantic_depth.max(self.semantic_depth);
        Ok(())
    }

    pub(super) fn enter_node_depth(&mut self) -> Result<()> {
        if self.node_depth >= MAX_SAFE_NODE_DEPTH {
            return Err(self.depth_failure(
                "expression_node",
                self.node_depth + 1,
                MAX_SAFE_NODE_DEPTH,
            ));
        }
        self.node_depth += 1;
        self.max_node_depth = self.max_node_depth.max(self.node_depth);
        Ok(())
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
                let guard = math::normalize(b::and(remaining.clone(), condition.clone()));
                let branch = math::normalize(b::and(path.clone(), guard.clone()));
                remaining = math::normalize(b::and(remaining, b::not(condition.clone())));
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
        let m = *self
            .methods
            .get(id)
            .unwrap_or_else(|| crate::preprocess::out_of_source(self.selected_entry.unwrap_or("Java execution"), id));
        need(m.params().len() == args.len(), "Java arity mismatch")?;
        let method_name = id.rsplit('.').next().unwrap_or(id);
        for ((t, name), v) in m.params().iter().zip(&mut args) {
            let expected = self.project.semantic_parameter_type(method_name, name, t)?;
            if expected == Ty::Text
                && matches!(v, Value::Term(Ty::Ref, e) if e.as_lit() == Some(&math::Lit::Null))
            {
                *v = Value::Term(Ty::Text, b::lit_null());
            }
            need(expected == v.ty(), "Java argument type mismatch")?;
        }
        let value = if let Some(name) = self.bindings.get(id).cloned() {
            self.call(&name, args, path)?
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
            value.ty() == self.project.result_type(m.return_type())?,
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
    pub(super) fn call(&mut self, name: &str, args: Vec<Value>, path: Expr) -> Result<Value> {
        self.enter_semantic_depth()?;
        let result = self.call_inner(name, args, path);
        self.semantic_depth -= 1;
        result
    }

    pub(super) fn call_inner(&mut self, name: &str, args: Vec<Value>, path: Expr) -> Result<Value> {
        self.budget()?;
        self.semantic_calls += 1;
        need(
            self.active.insert(format!("semantic:{name}")),
            "recursive semantic call",
        )?;
        let f = self
            .project
            .functions
            .get(name)
            .unwrap_or_else(|| crate::preprocess::out_of_source(self.selected_entry.unwrap_or("semantic execution"), name));
        need(f.params.len() == args.len(), "semantic arity mismatch")?;
        for (p, v) in f.params.iter().zip(&args) {
            need(
                Ty::parse(&p.typ)? == v.ty(),
                "semantic argument type mismatch",
            )?;
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
        let expected = Ty::parse(&f.returns)?;
        need(
            value.ty() == expected,
            &format!(
                "semantic result type mismatch: expected={} actual={}",
                expected.describe(),
                value.ty().describe()
            ),
        )?;
        self.active.remove(&format!("semantic:{name}"));
        Ok(value)
    }
    pub(super) fn node(&mut self, n: &Node, env: &mut Env, path: Expr) -> Result<Value> {
        self.enter_node_depth()?;
        let result = self.node_inner(n, env, path);
        self.node_depth -= 1;
        result
    }
}
