use super::FormatCtx;
use math::{builder as b, BinOpKind, Expr, SymVarOrigin};

// Count only eager uses of a closed, CASE-bearing sum over an acquired
// numeric input array. Never take a projection out of a guarded CASE or move
// a partial E/T observation. A row binder remains local to its query.
fn collect_closed_repetitions(value: &Expr, ctx: &FormatCtx, queries: &mut Vec<(Expr, usize)>) {
    use math::{domains::relation::RelationExpr, BinOpKind, RelationAggregateKind, SymVarOrigin};

    if let Some((RelationAggregateKind::Sum, RelationExpr::Base { name, alias }, Some(select), None)) =
        value.as_relation_aggregate()
    {
        let mut closed = !name.starts_with('$') && ctx.scalar_arrays.contains(name) && select.has_case_when();
        select.walk(&mut |part| {
            closed &= match part.as_sym_var() {
                Some(SymVarOrigin::Parameter { name, .. }) => !name.starts_with('$') && !name.contains('.')
                    && ctx.business_inputs.as_ref().is_some_and(|inputs| inputs.contains_key(name)),
                Some(_) => false,
                None => part.as_row_value().is_some_and(|row| row == alias)
                    || part.as_lit().is_some() || part.as_binop().is_some()
                    || part.as_unary_op().is_some()
                    || part.as_case_when().is_some_and(|branches|
                        branches.last().is_some_and(|(guard, _)| guard.as_bool() == Some(true))),
            };
        });
        if closed {
            if let Some((_, count)) = queries.iter_mut().find(|(expr, _)| expr == value) {
                *count += 1;
            } else {
                queries.push((value.clone(), 1));
            }
        }
    } else if let Some((BinOpKind::Add | BinOpKind::Sub | BinOpKind::Mul, left, right)) = value.as_binop() {
        collect_closed_repetitions(left, ctx, queries);
        collect_closed_repetitions(right, ctx, queries);
    }
}

pub(crate) fn format_value(value: &Expr, ctx: &mut FormatCtx) -> String {
    let mut queries = Vec::new();
    collect_closed_repetitions(value, ctx, &mut queries);
    let mut definitions = Vec::new();
    for (query, count) in queries {
        if count < 2 {
            continue;
        }
        // Formatter substitutions are expression-wide. If the same query
        // also exists outside the eager scope, do not capture that occurrence.
        let mut all = 0;
        value.walk(&mut |expr| all += usize::from(*expr == query));
        if all != count {
            continue;
        }
        let name = format!("$l_{}", definitions.len());
        definitions.push(format!(
            "LET {name} = {}",
            super::format_expr_with_ctx(&query, ctx)
        ));
        ctx.display_business_value(query, name);
    }
    let mut result = super::format_expr_with_ctx(value, ctx);
    if !definitions.is_empty() {
        result = format!("{}\nIN {result}", definitions.join("\n"));
        for _ in definitions {
            ctx.forget_business_value();
        }
    }
    result
}

// The mathematical CASE already selected its arms. In a minimum/maximum,
// display the test with its selected operand first, so WHEN and THEN agree.
pub(super) fn format_selection(branches: &[(Expr, Expr)], ctx: &mut FormatCtx) -> Option<String> {
    let [(guard, selected), (fallback, other)] = branches else {
        return None;
    };
    if fallback.as_bool() != Some(true) {
        return None;
    }
    let op = math::case_orientation::selection_comparison(guard, selected, other)?;
    if !matches!(op, math::BinOpKind::Lt | math::BinOpKind::Gt) {
        return None;
    }
    let first = super::format_expr_with_ctx(selected, ctx).replace('\n', "\n  ");
    let second = super::format_expr_with_ctx(other, ctx).replace('\n', "\n  ");
    let comparison = super::binop_display_token(op);
    Some(format!(
        "CASE\n  WHEN {first} {comparison} {second} THEN {first}\n  ELSE {second}\nEND"
    ))
}

/// A short-circuit candidate must be safe to observe, not merely algebraically
/// equivalent on total Boolean values.
fn receipts_available(value: &Expr, domain: Option<&Expr>, ctx: &FormatCtx) -> bool {
    let mut available = true;
    value.walk(&mut |node| {
        if !available {
            return;
        }
        if let Some(SymVarOrigin::External { binding_id, .. }) = node.as_sym_var() {
            if let Some(issued) = ctx.business_receipt_guards.get(binding_id) {
                available &= domain.is_some_and(|domain| b::path_implies(domain, issued));
            }
        }
    });
    available
}

fn selected_domain(op: BinOpKind, guard: &Expr, domain: Option<&Expr>) -> Expr {
    let selected = match op {
        BinOpKind::And => guard.clone(),
        BinOpKind::Or => math::normalize(b::not(guard.clone())),
        _ => unreachable!("only short-circuit Boolean operations have a selected domain"),
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
    if let Some((op @ (BinOpKind::And | BinOpKind::Or), left, right)) = value.as_binop() {
        return lazy_operands(op, left, right, domain, ctx).is_some();
    }
    false
}

fn lazy_operands<'a>(
    op: BinOpKind,
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
    if ctx.business_receipt_guards.is_empty() || !matches!(op, BinOpKind::And | BinOpKind::Or) {
        return None;
    }
    let domain = ctx.business_evaluation_guard.as_ref();
    if receipts_available(left, domain, ctx) && receipts_available(right, domain, ctx) {
        return None;
    }
    let (guard, value, selected) = lazy_operands(op, left, right, domain, ctx)?;
    let guard_text = super::format_expr_with_ctx(guard, ctx);
    let value_text = ctx.with_business_evaluation_guard(&selected, |ctx| {
        super::format_expr_with_ctx(value, ctx)
    });
    let (yes, no) = if op == BinOpKind::And {
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
