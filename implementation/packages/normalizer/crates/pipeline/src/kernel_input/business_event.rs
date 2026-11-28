//! Project source ports and settled host operations from online execution.
//! The settled action scope admits only fully awaited calls and explicit state writes.
use super::{
    business_entry::BusinessObservations,
    business_entry::EntryContext,
    realtime_state::{EffectReceipt, EffectOperation, TerminalOutcome},
    *,
};
use serde_json::json;

// Guards use the same PERS grammar and precedence as every value.
pub(super) fn condition_text(expr: &Expr, ctx: &mut crate::FormatCtx) -> String {
    crate::formatting::sql::format_array_failure_union(expr, ctx)
        .unwrap_or_else(|| crate::format_expr_with_ctx(expr, ctx))
}

fn bind_receipt<'a>(
    receipt: &'a EffectReceipt,
    guard: &Expr,
    identities: &BTreeMap<u64, math::ObservationIdentity>,
    issued: &mut BTreeSet<u64>,
    ctx: &mut crate::FormatCtx,
) -> Result<(&'a Expr, u64, String)> {
    let binding_id = receipt.binding_id();
    let position = ctx.receipt_count();
    need(issued.insert(binding_id), "duplicate operation receipt")?;
    ctx.bind_business_event(binding_id, position, identities[&binding_id].clone(), guard.clone());
    Ok((receipt.response(), binding_id, format!("$e_{position}")))
}

// A positive tag required by every path through a conjunction is proof;
// a tag under NOT, OR, or an unknown call is not.
fn requires_completion_tag(guard: &Expr, tag: &Expr) -> bool {
    guard == tag || matches!(guard.as_binop(), Some((BinOpKind::And, left, right))
        if requires_completion_tag(left, tag) || requires_completion_tag(right, tag))
}

pub(super) fn observations(
    project: &Project,
    entry: &EntryContext,
    result: &ExecutionResult,
    names: BTreeMap<String, String>,
) -> Result<BusinessObservations> {
    let states = result.source_state_bindings.iter()
        .map(|(&id, root)| (id, entry.qualified("E", root))).collect::<BTreeMap<_, _>>();
    let arrays = entry.scalar_array_symbols();
    let identities = entry.source_identity_names(result);
    let heap_keys = entry.source_identity_keys(result, &identities);
    let mut names = names;
    let alternatives = &result.terminal;
    let projected_record = result.source_record_return.as_deref();
    // The Java boundary already certified a total return of this one
    // declared, noncyclic record. Validate and render its contents, not
    // the internal reference token. Every other return keeps its checks.
    let returned = projected_record.map(|fields| b::array_lit(
        fields.iter().map(|(_, field)| field.clone()).collect()
    )).or_else(|| alternatives.value_on_normal_exit());
    let mut add_declared_paths = |expression: &Expr| {
        expression.walk(&mut |part| {
            if let Some(SymVarOrigin::Parameter { name, .. }) = part.as_sym_var() {
                if let Some(display) = entry.source_parameter_path(name) {
                    names.insert(name.clone(), display);
                }
            }
        });
    };
    if let Some(value) = &returned { add_declared_paths(value); }
    for alternative in alternatives.as_slice() {
        add_declared_paths(&alternative.guard);
        if let TerminalOutcome::Return(value) = &alternative.outcome { add_declared_paths(value); }
    }
    for event in result.realtime_effects.events() {
        add_declared_paths(&event.guard);
        for arg in event.arguments() { add_declared_paths(arg); }
    }
    let mut ctx = crate::FormatCtx::for_business(names.clone(), states, &result.source_state_bindings);
    if !project.direct_source { ctx.set_captured_parameters(entry.captured_parameters()); }
    ctx.set_business_identities(identities.clone(), heap_keys.clone());
    ctx.set_scalar_arrays(arrays.clone());
    ctx.set_record_arrays(entry.record_array_symbols());
    ctx.declare_nullable_columns(&result.source_tables);
    let mut issued = BTreeSet::new();
    let mut effects = Vec::new();
    let mut acquisitions = BTreeMap::new();
    let has_effects = result.realtime_effects.events().iter()
        .any(|event| !matches!(event.operation, EffectOperation::Acquire { .. }));
    let mut tables = result.source_tables.clone();
    let mut sql_exceptions = BTreeSet::new();
    let mut temporal_receipts = BTreeMap::new();
    let mut total_port_receipts = BTreeSet::new();
    let mut external_payloads = Vec::new();
    let mut receipt_throw_partitions = Vec::new();
    let validate_ids = |expr: &Expr, issued: &BTreeSet<u64>, temporals: Option<&BTreeMap<u64, usize>>| -> Result<()> {
        let mut closed = true;
        let mut heap_operands = BTreeMap::<i64, (usize, usize)>::new();
        expr.walk(&mut |node| {
            if let Some(relation) = node.as_relation()
                .or_else(|| node.as_relation_aggregate().map(|(_, relation, _, _)| relation))
            {
                relation.visit_acquisitions(&mut |id| { closed &= issued.contains(&id); });
            }
            if let Some((source, _, field, ty)) = math::domains::relation::positional::field_projection(node) {
                let domain = if ty == math::Primitive::Boolean { "bool" } else { "number" };
                closed &= entry.has_record_array_field(source, field, domain);
            }
            if let Some((math::RelationAggregateKind::NthValue, super::RelationExpr::RecordProjection { source, field }, _, _)) =
                node.as_relation_aggregate()
            {
                closed &= entry.has_record_array_field(source, field, "number");
            }
            if let Some(id) = node.as_heap_ref() {
                closed &= identities.contains_key(&id);
                heap_operands.entry(id).or_default().0 += 1;
            }
            if let Some((math::BinOpKind::Eq | math::BinOpKind::Ne, left, right)) = node.as_binop() {
                let allowed = |value: &Expr| value.is_null() || value.as_heap_ref().is_some_and(|id| identities.contains_key(&id))
                    || matches!(value.as_sym_var(), Some(SymVarOrigin::Parameter { name, primitive_type })
                        if primitive_type == "java_reference" && entry.parameter_type(name) == Some("unknown"));
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
                    SymVarOrigin::Temporal { binding_id, primitive_type } =>
                        matches!(primitive_type.as_str(), "integer" | b::RECORD_TEMPORAL_TAG)
                        && temporals.is_some_and(|names| names.contains_key(binding_id)),
                };
            }
        });
        // Named source references are permitted only as identity/null
        // comparison operands, never as naked objects in settled output.
        need(closed && heap_operands.values().all(|(actual, allowed)| actual == allowed),
            "unbound local or external value in settled output")
    };
    let event_identities = result.realtime_effects.observation_identities().collect::<BTreeMap<_, _>>();
    for event in result.realtime_effects.events() {
        validate_ids(&event.guard, &issued, None)?;
        for arg in event.arguments() {
            validate_ids(arg, &issued, None)?;
        }
        // Port/state guard text is rendered before registering any receipt.
        // SQL uses its original expression formatter after projecting the SQL item.
        let condition = (!matches!(&event.operation, EffectOperation::Sql { .. })
            && event.guard.as_bool() != Some(true)).then(|| condition_text(&event.guard, &mut ctx));
        let (label, mut item) = match &event.operation {
            EffectOperation::Acquire { acquisition, binding_id } => {
                use super::realtime_state::Acquisition;
                let label = format!("$a_{}", acquisitions.len());
                let mut item = match acquisition {
                    Acquisition::Clock => {
                        let mut read = json!({
                            "kind": "acquisition", "operation": "normalizer.intrinsics.N.currentTimeMillis", "type": "number"
                        });
                        if has_effects { read["afterS"] = json!(effects.len()); }
                        read
                    }
                    Acquisition::Sql { source, after } => {
                        let after = after.iter().map(|(receipt, guard)| {
                            let receipt = ctx.receipt_name(*receipt).ok_or_else(|| fail("SQL acquisition requires an earlier receipt"))?;
                            validate_ids(guard, &issued, None)?;
                            let mut dependency = json!({"result": receipt});
                            if guard.as_bool() != Some(true) {
                                dependency["when"] = json!(condition_text(guard, &mut ctx));
                            }
                            Ok(dependency)
                        }).collect::<Result<Vec<_>>>()?;
                        let mut acquisition = json!({"kind": "acquisition", "source": source});
                        if !after.is_empty() { acquisition["after"] = json!(after); }
                        acquisition
                    }
                };
                if let Some(guard) = condition { item["when"] = json!(guard); }
                need(issued.insert(*binding_id), "duplicate environment acquisition identity")?;
                ctx.bind_acquisition(*binding_id, label.clone(), event_identities[binding_id].clone(), event.guard.clone());
                acquisitions.insert(label, item);
                continue;
            }
            EffectOperation::Sql { statement: sql, receipt } => {
                let (response, binding_id, receipt) = bind_receipt(receipt, &event.guard, &event_identities, &mut issued, &mut ctx)?;
                let mut item = ctx.with_business_evaluation_guard(&event.guard, |ctx| {
                    sql.project(event.arguments(), &receipt, &mut tables, ctx)
                })?;
                if event.guard.as_bool() != Some(true) {
                    item["when"] = json!(crate::format_expr_with_ctx(&event.guard, &mut ctx));
                }
                // Atomic SQL writes have one declared failure type, so its
                // complement names this exact receipt's normal completion.
                let failed = math::domains::completion::is_throw(response.clone(), sql.exception());
                let normal = math::normalize(b::not(failed));
                ctx.bind_normal_completion(binding_id, normal.clone());
                ctx.display_business_observation(normal, format!("completion.is_return({receipt})"));
                ctx.display_business_observation(math::domains::completion::value(response.clone(), math::Primitive::Number), receipt.clone());
                sql_exceptions.insert(sql.exception());
                effects.push(item);
                continue;
            }
            EffectOperation::Port { id, receipt, completion: spec, source_identity } => {
                let contract = project
                    .ports
                    .get(id)
                    .ok_or_else(|| fail("missing external declaration"))?;
                let (name, ty) = entry.external_type(id)?;
                need(
                    spec.returns == ty,
                    "external business result disagrees with port type",
                )?;
                let (response, binding_id, output) = bind_receipt(receipt, &event.guard, &event_identities, &mut issued, &mut ctx)?;
                let variants = spec.throw_variants(response);
                if variants.len() > 1 {
                    // Only declared tags of this issued occurrence share a partition.
                    receipt_throw_partitions.push(variants.iter().map(|(_, failed, _)| failed.clone()).collect::<Vec<_>>());
                }
                for (class, failed, payload) in variants {
                    if let Some(payload) = payload.filter(|_| project.direct_source
                        && project.exceptions.get(&class).is_some_and(|ty| matches!(ty.as_str(), "text?" | "number")))
                    {
                        let domain = math::normalize(b::and(event.guard.clone(), failed));
                        external_payloads.push((class, payload, domain));
                    }
                }
                if event.guard.as_bool() == Some(true) && !spec.throwing() {
                    total_port_receipts.insert(binding_id);
                    if !result.temporal_definitions.is_empty() && spec.returns == Ty::Integer {
                        temporal_receipts.insert(binding_id, output.clone());
                    }
                }
                if spec.returns != Ty::Unit {
                    let value = spec.tagged_value(response)?;
                    ctx.display_business_observation(value, output.clone());
                }
                let normal = spec.normal_condition(response);
                if normal.as_bool().is_none() {
                    ctx.bind_normal_completion(binding_id, normal.clone());
                    ctx.display_business_observation(normal, format!("completion.is_return({output})"));
                }
                let args = ctx.with_business_evaluation_guard(&event.guard, |ctx| {
                    event.arguments().iter().enumerate().map(|(position, arg)| {
                        if project.direct_source {
                            if let (Ty::Record(fields), Some(values)) = (contract.parameter_type(position)?, arg.as_array_lit()) {
                                need(values.len() == fields.len(), "source port argument lost declared record fields")?;
                                let named = fields.into_keys().zip(values.iter().cloned()).collect::<Vec<_>>();
                                return Ok(crate::formatting::sql::business_value::format_record(arg, &named, ctx));
                            }
                        }
                        Ok(crate::format_expr_with_ctx(arg, ctx))
                    }).collect::<Result<Vec<_>>>()
                })?;
                let endpoint = if project.direct_source {
                    source_identity.clone()
                        .map_or_else(|| entry.direct_port_identity(&contract.endpoint), Ok)?
                } else {
                    contract.endpoint.clone()
                };
                (
                    "call",
                    json!({"external":endpoint,"args":args,"returns":name,"result":output}),
                )
            }
            EffectOperation::StateWrite { target, value } => {
                let value = match value.as_bool() {
                    Some(literal) => json!(literal),
                    None => json!(ctx.with_business_evaluation_guard(&event.guard, |ctx| {
                        crate::formatting::sql::business_value::format_value(value, ctx)
                    })),
                };
                use super::realtime_state::FieldWriteTarget;
                let target = match target {
                    FieldWriteTarget::EntryState(path) => entry.state_name(path)?,
                    FieldWriteTarget::Parameter(path) => entry.source_parameter_path(path)
                        .ok_or_else(|| fail("field write needs a declared parameter path"))?,
                };
                ("write", json!({"target":target,"value":value}))
            }
            EffectOperation::ArrayWrite { root, index, value } => {
                need(arrays.contains(root), "array write needs a declared P or E root")?;
                let name = names.get(root).ok_or_else(|| fail("unbound array write target"))?;
                let value = ctx.with_business_evaluation_guard(&event.guard, |ctx| {
                    crate::formatting::sql::business_value::format_value(value, ctx)
                });
                ("write", json!({"target":format!("{name}[{index}]"),"value":value}))
            }
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
        // Validate the receipts this choice/fold actually reads. An
        // unrelated guarded action cannot make an acquired fallback partial.
        entry.publish_temporals(result, returned.as_ref(), &names, |id, path| {
            entry.acquired_state_path(result, id, path)
                .or_else(|| if path.is_empty() { temporal_receipts.get(&id).cloned() } else { None })
        })?
    };
    let temporal_names = &publication.aliases;

    let mut exceptions: Vec<(Option<String>, Option<Expr>, Expr)> = Vec::new();
    for alternative in alternatives.as_slice() {
        validate_ids(&alternative.guard, &issued, None)?;
        match &alternative.outcome {
            TerminalOutcome::Throw { class, payload } => {
                if let Some(value) = payload {
                    need(
                        (project.direct_source && crate::normalized::native_text_exception_type(class)
                            && matches!(value.as_lit(), Some(math::Lit::Null | math::Lit::Str(_) | math::Lit::StrUnits(_))))
                            || (!project.direct_source
                                && project.functions.values().any(|function| {
                                    let Some((id, _)) = function.java.split_once('(') else { return false; };
                                    if !project.entry_contexts.get(id).is_some_and(|context| std::ptr::eq(context, entry)) {
                                        return false;
                                    }
                                    let failure = match &function.body {
                                        super::Node::GuardUnknownText { name, message }
                                            if class == "TypeError" && *value == message.to_expression() => {
                                            let input = b::param(name, math::ScalarSymbolType::UntrustedSourceValue);
                                            b::not(math::domains::source_type_guard::is_native_text(input))
                                        }
                                        super::Node::NativeTextFlow { flow } => flow.throw_domain(class, value),
                                        _ => return false,
                                    };
                                    b::path_implies(&alternative.guard, &math::normalize(failure))
                                }))
                            || external_payloads.iter().any(|(declared, observed, domain)|
                                declared == class && observed == value
                                    && b::path_implies(&alternative.guard, domain)),
                        "exception payload outside closed typed entry",
                    )?;
                    validate_ids(value, &issued, None)?;
                } else {
                    need(project.type_only_exception(class), "undeclared type-only exception")?;
                }
                if let Some((_, _, guard)) = exceptions.iter_mut()
                    .find(|(name, existing, _)| name.as_deref() == Some(class) && *existing == *payload)
                {
                    *guard = math::normalize(b::or(guard.clone(), alternative.guard.clone()));
                } else {
                    exceptions.push((Some(class.clone()), payload.clone(), alternative.guard.clone()));
                }
            }
            TerminalOutcome::ThrowValue(value) => {
                validate_ids(value, &issued, None)?;
                need(matches!(value.as_sym_var(), Some(SymVarOrigin::Parameter { name, .. })
                    if entry.parameter_type(name) == Some("unknown")), "classless throw has no approved input origin")?;
                if let Some((_, _, guard)) = exceptions.iter_mut()
                    .find(|(class, payload, _)| class.is_none() && payload.as_ref() == Some(value))
                {
                    *guard = math::normalize(b::or(guard.clone(), alternative.guard.clone()));
                } else {
                    exceptions.push((None, Some(value.clone()), alternative.guard.clone()));
                }
            }
            TerminalOutcome::Return(value) => {
                if projected_record.is_some() {
                    need(alternatives.total_return_value() == Some(value),
                        "record projection requires its proven total return")?;
                } else {
                    validate_ids(value, &issued, Some(temporal_names))?;
                }
            }
        }
    }
    let completion_classes = result.realtime_effects.total_failure_contracts().collect::<BTreeMap<_, _>>();
    let public_guards = crate::canonical_names::CanonicalNames {
        heap: &heap_keys, captured: ctx.captured_parameters(), completion_classes: Some(&completion_classes),
        external: Some(ctx.observation_identities()),
        total_temporals: None, temporals: None,
    };
    for (_, _, guard) in &mut exceptions {
        let stable_guard = math::try_normalize_with(guard.clone(), &public_guards)
            .map_err(|error| fail(error.to_string()))?;
        *guard = math::normalize::canonical_terminal_guard(stable_guard);
    }
    // PERS-05 applies equally to typed payloads and to type-only exits.
    // A proof about two tags concerns the same acquired receipt only.
    let reorderable = exceptions.iter().enumerate().all(|(i, (_, _, left_guard))| {
        exceptions[i + 1..].iter().all(|(_, _, right_guard)| {
            let tagged_disjoint = receipt_throw_partitions.iter().any(|tags|
                tags.iter().any(|left| requires_completion_tag(left_guard, left)
                    && tags.iter().any(|right| left != right
                        && requires_completion_tag(right_guard, right))));
            tagged_disjoint
                || math::normalize(b::and(left_guard.clone(), right_guard.clone())).as_bool() == Some(false)
        })
    });
    if reorderable {
        let identities = ctx.observation_identities();
        let public = crate::canonical_names::CanonicalNames {
            heap: &heap_keys, captured: ctx.captured_parameters(), completion_classes: None,
            external: Some(identities),
            total_temporals: None, temporals: None,
        };
        exceptions.sort_by_cached_key(|(name, payload, cond)| (
            name.clone(),
            payload.as_ref().map(|value| math::canonical_order_key(value, &public)),
            math::canonical_order_key(cond, &public),
        ));
    }
    let throws = exceptions
        .into_iter()
        .map(|(name, payload, cond)| {
            let guard = if name.as_ref().is_some_and(|name| sql_exceptions.contains(&name.as_str())) {
                crate::format_expr_with_ctx(&cond, &mut ctx)
            } else {
                condition_text(&cond, &mut ctx)
            };
            let mut record = json!({"when":guard});
            if let Some(name) = &name {
                record["type"] = json!(name);
            }
            if let Some(value) = payload {
                record["payload"] = json!(ctx.with_business_evaluation_guard(&cond, |ctx|
                    crate::format_expr_with_ctx(&value, ctx)));
                // A literal already carries its value type. The extra
                // field is only needed for a dynamically acquired payload.
                if let Some(ty) = name.as_ref().and_then(|name| project.exceptions.get(name))
                    .filter(|ty| ty.as_str() != "unit" && value.as_lit().is_none()) {
                    record["payload_type"] = json!(ty);
                } else if name.is_none() {
                    record["payload_type"] = json!("unknown");
                }
            }
            record
        })
        .collect();
    let returned = if let Some(returned) = returned {
        validate_ids(&returned, &issued, Some(temporal_names))?;
        // Supply settled observation identities to normalization, without
        // using the renderer's receipt ordinals as ordering keys.
        let mut reads_total_integer_field = false;
        if temporal_names.is_empty() {
            returned.walk(&mut |value| {
                if let Some(SymVarOrigin::External { binding_id, path, primitive_type }) = value.as_sym_var() {
                    reads_total_integer_field |= primitive_type == "integer" && !path.is_empty()
                        && total_port_receipts.contains(binding_id);
                }
            });
        }
        let returned = if completion_classes.is_empty() && temporal_names.is_empty() && !reads_total_integer_field
            && !returned.has_heap_ref()
        {
            returned
        } else {
            let observation_identities = ctx.observation_identities();
            super::temporal_graph::validate_reads(&returned, &result.temporal_definitions)?;
            let normalized = math::try_normalize_with(returned, &crate::canonical_names::CanonicalNames {
                heap: &heap_keys, captured: ctx.captured_parameters(), completion_classes: Some(&completion_classes),
                external: Some(observation_identities),
                total_temporals: Some(&publication.total), temporals: Some(temporal_names),
            }).map_err(|error| format!("[NORM-ANALYSIS-INCOMPLETE] {error}"))?;
            super::temporal_graph::validate_reads(&normalized, &result.temporal_definitions)?;
            normalized
        };
        let returned = if entry.return_type() == "bool" {
            math::normalize::compact_total_input_boolean_value(returned, &math::DefaultSortContext)
        } else { returned };
        ctx.set_temporal_names(publication.aliases);
        ctx.set_relation_slots(publication.slots);
        ctx.set_total_temporals(publication.total);
        let normal_domain = math::normalize(alternatives.as_slice().iter()
            .filter(|alternative| matches!(&alternative.outcome, TerminalOutcome::Return(_)))
            .fold(b::lit_bool(false), |domain, alternative| b::or(domain, alternative.guard.clone())));
        Some(ctx.with_business_evaluation_guard(&normal_domain, |ctx| -> Result<_> {
            Ok(if let Some(fields) = projected_record {
                need(returned.as_array_lit().is_some_and(|values| values.len() == fields.len()),
                    "projected record lost its declared fields")?;
                crate::formatting::sql::business_value::format_record(&returned, fields, ctx)
            } else if entry.return_type() == "void" {
                need(returned.is_null(), "non-void return in void event")?;
                "void".into()
            } else {
                crate::formatting::sql::business_value::format_value(&returned, ctx)
            })
        })?)
    } else {
        // The terminal alternatives were proved exhaustive; without a
        // return, every alternative is an observed business throw.
        None
    };
    Ok(BusinessObservations {
        tables,
        returned,
        effects,
        acquisitions,
        throws,
    })
}
