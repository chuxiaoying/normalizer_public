use super::FormatCtx;
use math::Expr;

// A repeated row query may have one lexical definition only if the query
// itself contains a CASE. A surrounding CASE does not qualify the query.
// Bindings belong to this R or S value; they never cross an operation boundary.
pub(crate) fn format_value(value: &Expr, ctx: &mut FormatCtx) -> String {
    let mut queries = Vec::<(Expr, usize)>::new();
    value.walk(&mut |node| {
        if node.as_relation_aggregate().is_some() && node.has_case_when() {
            if let Some((_, count)) = queries.iter_mut().find(|(expr, _)| expr == node) {
                *count += 1;
            } else {
                queries.push((node.clone(), 1));
            }
        }
    });
    let mut definitions = Vec::new();
    for (query, count) in queries {
        if count < 2 {
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

/// The shared math tree has no operation ledger: a Boolean expression can
/// have been flattened into AND/OR without knowledge of receipt existence.
/// At the boundary, restore the lazy guard when it is what makes a named
/// receipt exist. Unconditional receipts do not inhibit Boolean normal form.
pub(super) fn format_guarded_boolean(
    op: math::BinOpKind, left: &Expr, right: &Expr, ctx: &mut FormatCtx,
) -> Option<String> {
    use math::{builder as b, BinOpKind, SymVarOrigin};
    if !ctx.format_guarded_business_return || ctx.business_receipt_guards.is_empty() || !matches!(op, BinOpKind::And | BinOpKind::Or) {
        return None;
    }
    let receipts = |value: &Expr| {
        let mut ids = std::collections::BTreeSet::new();
        value.walk(&mut |node| {
            if let Some(SymVarOrigin::External { binding_id, .. }) = node.as_sym_var() {
                ids.insert(*binding_id);
            }
        });
        ids
    };
    for (guard, value) in [(left, right), (right, left)] {
        let guard_ids = receipts(guard);
        let value_ids = receipts(value);
        if guard_ids.iter().any(|id| ctx.business_receipt_guards.contains_key(id)) {
            continue;
        }
        let guarded: Vec<_> = value_ids.iter().filter_map(|id| ctx.business_receipt_guards.get(id)).collect();
        if guarded.is_empty() { continue; }
        let available = if op == BinOpKind::And { guard.clone() } else { math::normalize(b::not(guard.clone())) };
        if !guarded.iter().all(|issued| b::path_implies(&available, issued)) { continue; }
        let guard_text = super::format_expr_with_ctx(guard, ctx);
        let value_text = super::format_expr_with_ctx(value, ctx);
        let (yes, no) = if op == BinOpKind::And { (value_text.as_str(), "false") } else { ("true", value_text.as_str()) };
        if guard.as_sym_var().is_some() && !yes.contains('\n') && !no.contains('\n') {
            return Some(format!("CASE WHEN {guard_text} THEN {yes} ELSE {no} END"));
        }
        return Some(format!("CASE\n  WHEN {guard_text} THEN {yes}\n  ELSE {no}\nEND"));
    }
    None
}
