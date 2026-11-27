use super::FormatCtx;
use math::{builder as b, BinOpKind, Expr, SymVarOrigin};
fn direct_input(value: &Expr, ctx: &FormatCtx) -> bool {
    matches!(value.as_sym_var(), Some(SymVarOrigin::Parameter { name, primitive_type })
        if matches!(primitive_type.as_str(), "integer" | "boolean" | "text")
            && !name.starts_with('$') && !name.contains(['.', '[', ']'])
            && ctx.business_inputs.as_ref().is_some_and(|inputs| inputs.contains_key(name)))
}

fn total_value(value: &Expr, ctx: &FormatCtx, row: Option<&str>) -> bool {
    if value.as_lit().is_some() || direct_input(value, ctx) { return true; }
    if let Some(alias) = value.as_row_value() { return row == Some(alias); }
    if let Some((_, operand)) = value.as_unary_op() { return total_value(operand, ctx, row); }
    if let Some((_, left, right)) = value.as_binop() {
        return total_value(left, ctx, row) && total_value(right, ctx, row);
    }
    if let Some((arms, otherwise)) = value.as_total_case_when() {
        return arms.iter().all(|(guard, value)| total_value(guard, ctx, row) && total_value(value, ctx, row))
            && total_value(otherwise, ctx, row);
    }
    value.as_array_lit().is_some_and(|items| items.iter().all(|item| total_value(item, ctx, row)))
}

pub(super) fn closed_candidate(value: &Expr, ctx: &FormatCtx) -> bool {
    use math::{domains::relation::RelationExpr, RelationAggregateKind};
    if total_value(value, ctx, None) { return true; }
    // A SUM can be shared as a whole only over the exact same acquired
    // input sequence. Its row binder never participates in outer sharing.
    if let Some((RelationAggregateKind::Sum, RelationExpr::Base { name, alias, snapshot: None }, Some(select), None)) =
        value.as_relation_aggregate()
    {
        return !name.starts_with('$') && ctx.scalar_arrays.contains(name)
            && total_value(select, ctx, Some(alias));
    }
    false
}

fn with_value_plan(value: &Expr, ctx: &mut FormatCtx, render: impl FnOnce(&Expr, &mut FormatCtx) -> String) -> String {
    if ctx.business_inputs.is_none() || (!value.has_case_when() && ctx.relation_scope.slots.is_empty()) {
        return render(value, ctx);
    }
    let external = ctx.observation_identities().clone();
    let first = ctx.scoped_plans.is_none();
    if first {
        ctx.scoped_plans = Some(super::scoped_relation::ScopedPlans {
            external: external.clone(), plans: Default::default(),
        });
    }
    let domain = ctx.relation_scope.active.clone().unwrap_or_else(|| b::lit_bool(true));
    let plan = ctx.canonical_value_plan(value, &external, &domain, &ctx.relation_scope.member_order);
    let mut definitions = Vec::new();
    for (slot, expression) in plan.definitions.into_iter().enumerate() {
        let name = format!("$l_{slot}");
        definitions.push(format!("LET {name} = {}", super::format_expr_with_ctx(&expression, ctx)));
        ctx.display_business_value(expression, name);
    }
    // Choose bindings from the pre-rewrite value. A simplifying rule can
    // replace a member-dependent result without changing its acquisition.
    let result = ctx.in_relation_scope(value, |ctx| render(&plan.payload, ctx));
    for _ in &definitions { ctx.forget_business_value(); }
    if first { ctx.scoped_plans = None; }
    if definitions.is_empty() { result }
    else { format!("{}\nIN {result}", definitions.join("\n")) }
}

pub(crate) fn format_value(value: &Expr, ctx: &mut FormatCtx) -> String {
    with_value_plan(value, ctx, super::format_expr_with_ctx)
}

/// A short-circuit candidate must be safe to observe, not merely algebraically
/// equivalent on total Boolean values.
fn receipts_available(value: &Expr, domain: Option<&Expr>, ctx: &FormatCtx) -> bool {
    use math::domains::completion;
    let proves = |requirement: &Expr| requirement.as_bool() == Some(true)
        || domain.is_some_and(|active| b::path_implies(active, requirement));
    let mut available = true;
    value.walk_pruned(&mut |node| {
        if !available { return false; }
        if let Some(SymVarOrigin::External { binding_id, .. }) = node.as_sym_var() {
            if let Some(issued) = ctx.business_receipt_guards.get(binding_id) {
                available &= proves(issued);
            }
        }
        if let Some((key, args)) = node.as_call() {
            let registered = math::func_name(key);
            match (completion::output_name(&registered), args) {
                ("completion.value", [response]) => {
                    if let Some(SymVarOrigin::External { binding_id, .. }) = response.as_sym_var() {
                        if let Some(normal) = ctx.business_normal_completions.get(binding_id) {
                            available &= proves(normal);
                        }
                    }
                }
                ("completion.payload", _) => {
                    if let Some((_, receipt, class)) = completion::class_observation(&registered, args) {
                        available &= proves(&completion::is_throw(receipt.clone(), class));
                    }
                }
                _ => {}
            }
        }
        available
    });
    available
}

#[derive(Clone, Copy)]
enum ShortCircuit {
    And,
    Or,
}

impl ShortCircuit {
    fn from_op(op: BinOpKind) -> Option<Self> {
        match op {
            BinOpKind::And => Some(Self::And),
            BinOpKind::Or => Some(Self::Or),
            _ => None,
        }
    }
}

fn selected_domain(op: ShortCircuit, guard: &Expr, domain: Option<&Expr>) -> Expr {
    let selected = match op {
        ShortCircuit::And => guard.clone(),
        ShortCircuit::Or => math::normalize(b::not(guard.clone())),
    };
    match domain {
        Some(domain) => math::normalize(b::and(domain.clone(), selected)),
        None => selected,
    }
}

fn is_safe(value: &Expr, domain: Option<&Expr>, ctx: &FormatCtx) -> bool {
    if receipts_available(value, domain, ctx) {
        return true;
    }
    value.as_binop().is_some_and(|(op, left, right)| {
        ShortCircuit::from_op(op).is_some_and(|op| lazy_operands(op, left, right, domain, ctx).is_some())
    })
}

fn lazy_operands<'a>(
    op: ShortCircuit,
    left: &'a Expr,
    right: &'a Expr,
    domain: Option<&Expr>,
    ctx: &FormatCtx,
) -> Option<(&'a Expr, &'a Expr, Expr)> {
    for (guard, value) in [(left, right), (right, left)] {
        if is_safe(guard, domain, ctx) {
            let selected = selected_domain(op, guard, domain);
            if is_safe(value, Some(&selected), ctx) {
                return Some((guard, value, selected));
            }
        }
    }
    None
}

/// The shared math tree has no operation ledger: a Boolean expression can
/// have been flattened into AND/OR without knowledge of receipt existence.
/// Restore lazy evaluation wherever a named receipt depends on that guard.
/// A dominating operation guard can already establish receipt availability.
pub(crate) fn format_guarded_boolean(
    op: BinOpKind,
    left: &Expr,
    right: &Expr,
    ctx: &mut FormatCtx,
) -> Option<String> {
    if ctx.business_receipt_guards.is_empty() && ctx.business_normal_completions.is_empty() {
        return None;
    }
    let op = ShortCircuit::from_op(op)?;
    let domain = ctx.business_evaluation_guard.as_ref();
    // Issuance is not the same as successful completion. If both operands
    // are already available, no extra CASE is necessary.
    if receipts_available(left, domain, ctx) && receipts_available(right, domain, ctx) {
        return None;
    }
    let (guard, value, selected) = lazy_operands(op, left, right, domain, ctx)?;
    let guard_text = super::format_expr_with_ctx(guard, ctx);
    let value_text = ctx.with_business_evaluation_guard(&selected, |ctx| {
        super::format_expr_with_ctx(value, ctx)
    });
    let (yes, no) = if matches!(op, ShortCircuit::And) {
        (value_text.as_str(), "false")
    } else {
        ("true", value_text.as_str())
    };
    if guard.as_sym_var().is_some() && !yes.contains('\n') && !no.contains('\n') {
        return Some(format!("CASE WHEN {guard_text} THEN {yes} ELSE {no} END"));
    }
    let guard_text = guard_text.replace('\n', "\n  ");
    let yes = yes.replace('\n', "\n  ");
    let no = no.replace('\n', "\n  ");
    Some(format!("CASE\n  WHEN {guard_text} THEN {yes}\n  ELSE {no}\nEND"))
}

pub(crate) fn format_record(expression: &Expr, fields: &[(String, Expr)], ctx: &mut FormatCtx) -> String {
    with_value_plan(expression, ctx, |value, ctx| {
        let values = value.as_array_lit().expect("declared record projection");
        let lines: Vec<_> = fields.iter().zip(values).map(|((name, _), value)| {
            let value = super::format_expr_with_ctx(value, ctx);
            format!("  {name}: {}", value.replace('\n', "\n  "))
        }).collect();
        format!("{{\n{}\n}}", lines.join(",\n"))
    })
}
