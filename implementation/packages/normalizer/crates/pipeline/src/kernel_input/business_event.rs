//! Project source ports and settled host operations from online execution.
//! The settled action scope admits only fully awaited calls and explicit state writes.
use super::{
    business_entry::BusinessObservations,
    business_entry::EntryContext,
    realtime_state::{CompletionOutcome, EffectEvent, EffectOperation},
    *,
};
use serde_json::json;

// Guards use the PERS condition grammar. Parentheses preserve the math tree,
// and operands remain in their original evaluation order.
fn condition_text(expr: &Expr, ctx: &mut crate::FormatCtx) -> String {
    fn render(expr: &Expr, parent: u8, ctx: &mut crate::FormatCtx) -> String {
        if let Some(bound) = crate::formatting::sql::format_array_failure_union(expr, ctx) {
            return bound;
        }
        if let Some((op, left, right)) = expr.as_binop() {
            let (rank, symbol) = match op {
                BinOpKind::Or => (1, "or"),
                BinOpKind::And => (2, "and"),
                _ => return crate::format_expr_with_ctx(expr, ctx),
            };
            if let Some(guarded) =
                crate::formatting::sql::business_value::format_guarded_boolean(op, left, right, ctx)
            {
                return guarded;
            }
            let group = format!(
                "{} {symbol} {}",
                render(left, rank, ctx),
                render(right, rank, ctx)
            );
            return if rank < parent {
                format!("({group})")
            } else {
                group
            };
        }
        crate::format_expr_with_ctx(expr, ctx)
    }
    render(expr, 0, ctx)
}

fn bind_receipt<'a>(
    event: &'a EffectEvent,
    issued: &mut BTreeSet<u64>,
    ctx: &mut crate::FormatCtx,
) -> Result<(&'a Expr, String)> {
    let response = event.result.as_ref().ok_or_else(|| fail("missing operation receipt"))?;
    let Some(SymVarOrigin::External { binding_id, .. }) = response.as_sym_var() else {
        return Err(fail("invalid operation receipt"));
    };
    let position = issued.len();
    need(issued.insert(*binding_id), "duplicate operation receipt")?;
    ctx.bind_business_event(*binding_id, position, event.guard.clone());
    Ok((response, format!("$e_{position}")))
}

pub(super) fn observations(
    project: &Project,
    entry: &EntryContext,
    result: &ExecutionResult,
    names: BTreeMap<String, String>,
) -> Result<BusinessObservations> {
    result.check_visible_heap()?;
    let states = result.source_state_bindings.iter()
        .map(|(&id, root)| (id, entry.qualified("E", root))).collect::<BTreeMap<_, _>>();
    let arrays = entry.scalar_array_symbols();
    let mut ctx = crate::FormatCtx::for_business(names.clone(), states);
    ctx.set_scalar_arrays(arrays.clone());
    let mut issued = BTreeSet::new();
    let mut effects = Vec::new();
    let mut tables = result.source_tables.clone();
    let mut sql_exceptions = BTreeSet::new();
    let validate_ids = |expr: &Expr, issued: &BTreeSet<u64>| -> Result<()> {
        let mut closed = true;
        expr.walk(&mut |node| {
            closed &= node.as_heap_ref().is_none();
            if let Some(origin) = node.as_sym_var() {
                closed &= match origin {
                    SymVarOrigin::Parameter { name, .. } => names.contains_key(name),
                    SymVarOrigin::External { binding_id, path, .. } => issued.contains(binding_id)
                        || result.source_state_bindings.get(binding_id)
                            .is_some_and(|root| entry.source_state_path(root, path)),
                    _ => false,
                };
            }
        });
        need(closed, "unbound local or external value in settled output")
    };
    for event in result.realtime_effects.events() {
        validate_ids(&event.guard, &issued)?;
        for arg in &event.args {
            validate_ids(arg, &issued)?;
        }
        if let EffectOperation::Sql(sql) = &event.operation {
            let (response, receipt) = bind_receipt(event, &mut issued, &mut ctx)?;
            let mut item = ctx.with_business_evaluation_guard(&event.guard, |ctx| {
                sql.project(&event.args, &receipt, &mut tables, ctx)
            })?;
            if event.guard.as_bool() != Some(true) {
                item["when"] = json!(crate::format_expr_with_ctx(&event.guard, &mut ctx));
            }
            // Atomic SQL writes have one declared failure type, so its
            // complement names this exact receipt's normal completion.
            let failed = math::domains::completion::is_throw(response.clone(), sql.exception());
            ctx.display_business_value(math::normalize(b::not(failed)), format!("completion.is_return({receipt})"));
            sql_exceptions.insert(sql.exception());
            effects.push(item);
            continue;
        }
        let condition =
            (event.guard.as_bool() != Some(true)).then(|| condition_text(&event.guard, &mut ctx));
        let (label, mut item) = match &event.operation {
            EffectOperation::Port(id) => {
                let contract = project
                    .ports
                    .get(id)
                    .ok_or_else(|| fail("missing external declaration"))?;
                let (name, ty) = entry.external_type(id)?;
                let spec = contract.completion_spec(project)?;
                need(
                    spec.returns == ty,
                    "external business result disagrees with port type",
                )?;
                let (response, output) = bind_receipt(event, &mut issued, &mut ctx)?;
                if spec.returns != Ty::Unit {
                    let value = spec.awaited_value(response)?;
                    ctx.display_business_value(value, output.clone());
                }
                let normal = spec.normal_condition(response);
                if normal.as_bool().is_none() {
                    ctx.display_business_value(normal, format!("completion.is_return({output})"));
                }
                let args: Vec<_> = ctx.with_business_evaluation_guard(&event.guard, |ctx| {
                    event.args.iter().map(|arg| crate::format_expr_with_ctx(arg, ctx)).collect()
                });
                (
                    "call",
                    json!({"external":contract.endpoint,"args":args,"returns":name,"result":output}),
                )
            }
            EffectOperation::StateWrite(target) => {
                let [value] = event.args.as_slice() else {
                    return Err(fail("missing write value"));
                };
                let value = match value.as_bool() {
                    Some(literal) => json!(literal),
                    None => json!(ctx.with_business_evaluation_guard(&event.guard, |ctx| {
                        crate::formatting::sql::business_value::format_value(value, ctx)
                    })),
                };
                (
                    "write",
                    json!({"target":entry.state_name(target)?,"value":value}),
                )
            }
            EffectOperation::ArrayWrite { root, index } => {
                let [value] = event.args.as_slice() else { return Err(fail("missing array write value")); };
                need(arrays.contains(root), "array write needs a declared P or E root")?;
                let name = names.get(root).ok_or_else(|| fail("unbound array write target"))?;
                let value = ctx.with_business_evaluation_guard(&event.guard, |ctx| {
                    crate::formatting::sql::business_value::format_value(value, ctx)
                });
                ("write", json!({"target":format!("{name}[{index}]"),"value":value}))
            }
            _ => return Err(fail("unsupported event in port/state sequence")),
        };
        if let Some(guard) = condition {
            item["when"] = json!(guard);
        }
        let mut wrapper = json!({});
        wrapper[label] = item;
        effects.push(wrapper);
    }
    let alternatives = result
        .realtime_completion
        .terminal_alternatives()
        .ok_or_else(|| fail("entry has incomplete control flow"))?;
    let mut exceptions = BTreeMap::new();
    let mut return_guard = b::lit_bool(false);
    for alternative in alternatives {
        validate_ids(&alternative.guard, &issued)?;
        match alternative.outcome {
            CompletionOutcome::Throw { class, payload } => {
                need(
                    project.type_only_exception(&class) && payload == b::lit_null(),
                    "exception payload outside closed type-only entry",
                )?;
                let condition = exceptions
                    .entry(class)
                    .or_insert_with(|| b::lit_bool(false));
                *condition = math::normalize(b::or(condition.clone(), alternative.guard));
            }
            CompletionOutcome::Return(value) => {
                validate_ids(&value, &issued)?;
                return_guard = b::or(return_guard, alternative.guard);
            }
            CompletionOutcome::Continue => return Err(fail("incomplete settled return")),
        }
    }
    let throws = exceptions
        .into_iter()
        .map(|(name, cond)| {
            let guard = if sql_exceptions.contains(name.as_str()) {
                crate::format_expr_with_ctx(&cond, &mut ctx)
            } else {
                condition_text(&cond, &mut ctx)
            };
            json!({"type":name,"when":guard})
        })
        .collect();
    let returned = if result.realtime_completion.is_always_throwing() {
        // A fully covered, terminal throw has no value, not a null value.
        "⊥".into()
    } else {
        let returned = result.realtime_completion.value_on_normal_exit()
            .ok_or_else(|| fail("no normal settled return"))?;
        validate_ids(&returned, &issued)?;
        if !arrays.is_empty() {
            ctx.set_array_exit_guard(math::normalize(return_guard));
        }
        if entry.return_type() == "void" {
            need(returned.is_null(), "non-void return in void event")?;
            "void".into()
        } else {
            crate::formatting::sql::business_value::format_value(&returned, &mut ctx)
        }
    };
    Ok(BusinessObservations {
        tables,
        returned,
        effects,
        throws,
    })
}
