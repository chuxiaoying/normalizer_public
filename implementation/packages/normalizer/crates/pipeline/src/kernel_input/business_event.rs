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
pub(super) fn condition_text(expr: &Expr, ctx: &mut crate::FormatCtx) -> String {
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
    let identities = entry.source_identity_names(result);
    let mut names = names;
    let returned = result.realtime_completion.value_on_normal_exit()
        .ok_or_else(|| fail("no normal settled return"))?;
    let mut add_declared_paths = |expression: &Expr| {
        expression.walk(&mut |part| {
            if let Some(SymVarOrigin::Parameter { name, .. }) = part.as_sym_var() {
                if let Some(display) = entry.source_parameter_path(name) {
                    names.insert(name.clone(), display);
                }
            }
        });
    };
    add_declared_paths(&returned);
    let alternatives = result.realtime_completion.terminal_alternatives()
        .ok_or_else(|| fail("entry has incomplete control flow"))?;
    for alternative in &alternatives {
        add_declared_paths(&alternative.guard);
        if let CompletionOutcome::Return(value) = &alternative.outcome { add_declared_paths(value); }
    }
    for event in result.realtime_effects.events() {
        add_declared_paths(&event.guard);
        for arg in &event.args { add_declared_paths(arg); }
    }
    let mut ctx = crate::FormatCtx::for_business(names.clone(), states);
    ctx.set_business_identities(identities.clone());
    ctx.set_scalar_arrays(arrays.clone());
    ctx.set_record_arrays(entry.record_array_symbols());
    ctx.declare_nullable_columns(&result.source_tables);
    let mut issued = BTreeSet::new();
    let mut effects = Vec::new();
    let mut tables = result.source_tables.clone();
    let mut sql_exceptions = BTreeSet::new();
    let mut temporal_receipts = BTreeMap::new();
    let mut external_payloads = Vec::new();
    let validate_ids = |expr: &Expr, issued: &BTreeSet<u64>, temporals: Option<&BTreeMap<u64, usize>>| -> Result<()> {
        let mut closed = true;
        let mut heap_operands = BTreeMap::<i64, (usize, usize)>::new();
        expr.walk(&mut |node| {
            if let Some(id) = node.as_heap_ref() {
                closed &= identities.contains_key(&id);
                heap_operands.entry(id).or_default().0 += 1;
            }
            if let Some((math::BinOpKind::Eq | math::BinOpKind::Ne, left, right)) = node.as_binop() {
                let allowed = |value: &Expr| value.is_null() || value.as_heap_ref().is_some_and(|id| identities.contains_key(&id));
                if allowed(left) && allowed(right) {
                    for id in [left, right].iter().filter_map(|value| value.as_heap_ref()) {
                        heap_operands.entry(id).or_default().1 += 1;
                    }
                }
            }
            if let Some(origin) = node.as_sym_var() {
                closed &= match origin {
                    SymVarOrigin::Parameter { name, .. } => names.contains_key(name),
                    SymVarOrigin::External { binding_id, path, .. } => issued.contains(binding_id)
                        || result.source_state_bindings.get(binding_id)
                            .is_some_and(|root| entry.source_state_path(root, path)),
                    SymVarOrigin::Temporal { binding_id, primitive_type } => primitive_type == "integer"
                        && temporals.is_some_and(|names| names.contains_key(binding_id)),
                };
            }
        });
        // Named source references are permitted only as identity/null
        // comparison operands, never as naked objects in settled output.
        need(closed && heap_operands.values().all(|(actual, allowed)| actual == allowed),
            "unbound local or external value in settled output")
    };
    for event in result.realtime_effects.events() {
        validate_ids(&event.guard, &issued, None)?;
        for arg in &event.args {
            validate_ids(arg, &issued, None)?;
        }
        if let EffectOperation::Sql(sql) = &event.operation {
            let (response, receipt) = bind_receipt(event, &mut issued, &mut ctx)?;
            let mut item = sql.project(&event.args, &receipt, &mut tables, &mut ctx)?;
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
                for (class, failed, payload) in spec.throw_variants(response) {
                    if let Some(payload) = payload.filter(|_| project.direct_source
                        && project.exceptions.get(&class).is_some_and(|ty| matches!(ty.as_str(), "text?" | "number")))
                    {
                        let domain = math::normalize(b::and(event.guard.clone(), failed));
                        external_payloads.push((class, payload, domain));
                    }
                }
                if !result.temporal_definitions.is_empty() && event.guard.as_bool() == Some(true)
                    && spec.returns == Ty::Integer && !spec.throwing()
                {
                    let Some(SymVarOrigin::External { binding_id, .. }) = response.as_sym_var() else {
                        return Err(fail("missing already bound temporal receipt"));
                    };
                    temporal_receipts.insert(*binding_id, output.clone());
                }
                if spec.returns != Ty::Unit {
                    let value = spec.awaited_value(response)?;
                    ctx.display_business_value(value, output.clone());
                }
                let normal = spec.normal_condition(response);
                if normal.as_bool().is_none() {
                    ctx.display_business_value(normal, format!("completion.is_return({output})"));
                }
                let args: Vec<_> = event
                    .args
                    .iter()
                    .map(|arg| crate::format_expr_with_ctx(arg, &mut ctx))
                    .collect();
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
                    None => json!(crate::formatting::sql::business_value::format_value(value, &mut ctx)),
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
                let value = crate::formatting::sql::business_value::format_value(value, &mut ctx);
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

    let publication = if result.temporal_definitions.is_empty() {
        super::temporal_graph::Publication::default()
    } else {
        need(temporal_receipts.len() == effects.len(),
            "temporal definitions with effects require unconditional nonthrowing integer receipts")?;
        entry.publish_temporals(result, &returned, &names, |id, path| {
            entry.acquired_state_path(result, id, path)
                .or_else(|| if path.is_empty() { temporal_receipts.get(&id).cloned() } else { None })
        })?
    };
    let temporal_names = &publication.aliases;

    let mut exceptions: Vec<(String, Option<Expr>, Expr)> = Vec::new();
    for alternative in alternatives {
        validate_ids(&alternative.guard, &issued, None)?;
        match alternative.outcome {
            CompletionOutcome::Throw { class, payload } => {
                if let Some(value) = &payload {
                    need(
                        (project.direct_source && class == crate::normalized::RUNTIME_EXCEPTION
                            && matches!(value.as_lit(), Some(math::Lit::Null | math::Lit::Str(_))))
                            || external_payloads.iter().any(|(declared, observed, domain)|
                                declared == &class && observed == value
                                    && b::path_implies(&alternative.guard, domain)),
                        "exception payload outside closed typed entry",
                    )?;
                    validate_ids(value, &issued, None)?;
                } else {
                    need(project.type_only_exception(&class), "undeclared type-only exception")?;
                }
                if let Some((_, _, guard)) = exceptions.iter_mut()
                    .find(|(name, existing, _)| *name == class && *existing == payload)
                {
                    *guard = math::normalize(b::or(guard.clone(), alternative.guard));
                } else {
                    exceptions.push((class, payload, alternative.guard));
                }
            }
            CompletionOutcome::Return(value) => {
                validate_ids(&value, &issued, Some(temporal_names))?;
            }
            CompletionOutcome::Continue => return Err(fail("incomplete settled return")),
        }
    }
    if exceptions.iter().any(|(_, payload, _)| payload.is_some()) {
        for (i, (_, left, left_guard)) in exceptions.iter().enumerate() {
            for (_, right, right_guard) in &exceptions[i + 1..] {
                if left.is_some() || right.is_some() {
                    need(math::normalize(b::and(left_guard.clone(), right_guard.clone())).as_bool() == Some(false),
                        "typed exception alternatives lack a disjoint terminal guard proof")?;
                }
            }
        }
        let public_names = ctx.published_external_names();
        let public = crate::canonical_names::CanonicalNames {
            heap: &BTreeMap::new(),
            external: Some(&public_names),
            total_temporals: None, temporals: None,
        };
        exceptions.sort_by_cached_key(|(name, payload, cond)| (
            name.clone(),
            payload.as_ref().map(|value| math::canonical_order_key(value, &public)),
            math::canonical_order_key(cond, &public),
        ));
    } else {
        exceptions.sort_by(|a, b| a.0.cmp(&b.0));
    }
    let throws = exceptions
        .into_iter()
        .map(|(name, payload, cond)| {
            let guard = if sql_exceptions.contains(name.as_str()) {
                crate::format_expr_with_ctx(&cond, &mut ctx)
            } else {
                condition_text(&cond, &mut ctx)
            };
            let mut record = json!({"type":name,"when":guard});
            if let Some(value) = payload {
                record["payload"] = json!(crate::format_expr_with_ctx(&value, &mut ctx));
                if let Some(ty) = project.exceptions.get(&name).filter(|ty| ty.as_str() != "unit") {
                    record["payload_type"] = json!(ty);
                }
            }
            record
        })
        .collect();
    validate_ids(&returned, &issued, Some(temporal_names))?;
    let returned = if temporal_names.is_empty() {
        returned
    } else {
        let external_names = ctx.published_external_names();
        super::temporal_graph::validate_reads(&returned, &result.temporal_definitions)?;
        let normalized = math::try_normalize_with(returned, &crate::canonical_names::CanonicalNames {
            heap: &BTreeMap::new(),
            external: Some(&external_names),
            total_temporals: Some(&publication.total), temporals: Some(temporal_names),
        }).map_err(|error| fail(error.to_string()))?;
        super::temporal_graph::validate_reads(&normalized, &result.temporal_definitions)?;
        normalized
    };
    ctx.set_temporal_names(publication.aliases);
    ctx.protect_business_return();
    let returned = if entry.return_type() == "void" {
        need(returned.is_null(), "non-void return in void event")?;
        "void".into()
    } else {
        crate::formatting::sql::business_value::format_value(&returned, &mut ctx)
    };
    Ok(BusinessObservations {
        tables,
        returned,
        effects,
        throws,
        definitions: publication.rows,
    })
}
