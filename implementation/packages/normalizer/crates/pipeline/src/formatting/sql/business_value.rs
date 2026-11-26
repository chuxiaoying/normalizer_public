use super::FormatCtx;
use math::{builder as b, BinOpKind, Expr, SymVarOrigin};
use std::collections::HashMap;

struct Repetition {
    query: Expr,
    eager: usize,
}

// Count only eager uses of a closed, CASE-bearing sum over an acquired
// numeric input array. Never take a projection out of a guarded CASE or move
// a partial E/T observation. A row binder remains local to its query.
fn collect_closed_repetitions(value: &Expr, ctx: &FormatCtx, queries: &mut Vec<Repetition>) {
    use math::{domains::relation::RelationExpr, BinOpKind, RelationAggregateKind, SymVarOrigin};

    if let Some((RelationAggregateKind::Sum, RelationExpr::Base { name, alias }, Some(select), None)) =
        value.as_relation_aggregate()
    {
        if let Some(repetition) = queries.iter_mut().find(|repetition| &repetition.query == value) {
            repetition.eager += 1;
            return;
        }
        if name.starts_with('$') || !ctx.scalar_arrays.contains(name) || !select.has_case_when() {
            return;
        }
        let mut closed = true;
        select.walk(&mut |part| {
            closed &= match part.as_sym_var() {
                Some(SymVarOrigin::Parameter { name, .. }) => !name.starts_with('$') && !name.contains('.')
                    && ctx.business_inputs.as_ref().is_some_and(|inputs| inputs.contains_key(name)),
                Some(_) => false,
                None => part.as_row_value().is_some_and(|row| row == alias)
                    || part.as_lit().is_some() || part.as_binop().is_some()
                    || part.as_unary_op().is_some()
                    || part.as_total_case_when().is_some(),
            };
        });
        if closed {
            queries.push(Repetition { query: value.clone(), eager: 1 });
        }
    } else if let Some((BinOpKind::Add | BinOpKind::Sub | BinOpKind::Mul, left, right)) = value.as_binop() {
        collect_closed_repetitions(left, ctx, queries);
        collect_closed_repetitions(right, ctx, queries);
    }
}

pub(crate) fn format_value(value: &Expr, ctx: &mut FormatCtx) -> String {
    let mut queries = Vec::new();
    collect_closed_repetitions(value, ctx, &mut queries);
    queries.retain(|repetition| repetition.eager > 1);
    // Count all occurrences together: formatter substitutions are
    // expression-wide, so none of a query's occurrences may be guarded.
    let mut total_counts = vec![0; queries.len()];
    if !queries.is_empty() {
        let indices: HashMap<_, _> = queries.iter().enumerate()
            .map(|(index, repetition)| (&repetition.query, index))
            .collect();
        value.walk(&mut |expr| {
            if let Some(&index) = indices.get(expr) {
                total_counts[index] += 1;
            }
        });
    }
    let mut definitions = Vec::new();
    for (Repetition { query, eager }, total) in queries.into_iter().zip(total_counts) {
        if eager != total {
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

#[cfg(test)]
mod let_scope_contract {
    use super::*;
    use math::{domains::relation as r, ScalarSymbolType};
    use std::collections::{BTreeMap, BTreeSet};

    #[test]
    fn a_third_guarded_use_blocks_a_global_binding_for_two_eager_uses() {
        let inputs = ["enabled", "extra", "publish", "values"].into_iter()
            .map(|name| (name.to_owned(), name.to_owned())).collect::<BTreeMap<_, _>>();
        let mut ctx = FormatCtx::for_business(inputs, BTreeMap::new());
        ctx.set_scalar_arrays(BTreeSet::from(["values".to_owned()]));
        let zero = b::lit_int(0);
        let offset = b::case_when(vec![(
            b::param("enabled", ScalarSymbolType::Boolean),
            b::param("extra", ScalarSymbolType::Integer),
        )], zero.clone());
        let query = r::relation_sum(r::RelationExpr::base("values", "item"),
            b::add(b::acquired_integer_row("item"), offset));
        let repeated = b::mul(query.clone(), query.clone());
        let eager = format_value(&repeated, &mut ctx);
        assert!(eager.starts_with("LET $l_0 = "));
        assert!(eager.ends_with("IN $l_0 * $l_0"));

        let guarded = b::case_when(vec![(
            b::param("publish", ScalarSymbolType::Boolean), query,
        )], zero);
        let mixed = format_value(&b::add(repeated, guarded), &mut ctx);
        assert!(!mixed.contains("$l_"));
        assert_eq!(mixed.matches("SUM(").count(), 3);
    }

    #[test]
    fn an_unrelated_guarded_receipt_does_not_change_pure_boolean_spelling() {
        let param = |name| b::param(name, ScalarSymbolType::Boolean);
        let enabled = param("enabled");
        let valid = param("valid");
        let mut ctx = FormatCtx::for_business(BTreeMap::new(), BTreeMap::new());
        ctx.bind_business_event(0, 0, enabled.clone());
        assert!(format_guarded_boolean(BinOpKind::And, &enabled, &valid, &mut ctx).is_none());

        let receipt = b::sym_e(0, ScalarSymbolType::Boolean);
        let actual = format_guarded_boolean(BinOpKind::And, &enabled, &receipt, &mut ctx)
            .expect("the conditional receipt still requires its guard");
        assert!(actual.starts_with("CASE WHEN") && actual.contains("$e_0"));
    }
}

// The mathematical CASE already selected its arms. In a minimum/maximum,
// display the test with its selected operand first, so WHEN and THEN agree.
pub(super) fn format_selection(expr: &Expr, ctx: &mut FormatCtx) -> Option<String> {
    let ([(guard, selected)], other) = expr.as_total_case_when()? else {
        return None;
    };
    let op = math::case_orientation::selection_comparison(guard, selected, other)?;
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
    if ctx.business_receipt_guards.is_empty() {
        return None;
    }
    let op = ShortCircuit::from_op(op)?;
    let mut guarded_receipt = false;
    for operand in [left, right] {
        operand.walk(&mut |part| {
            guarded_receipt |= matches!(
                part.as_sym_var(),
                Some(SymVarOrigin::External { binding_id, .. })
                    if ctx.business_receipt_guards.contains_key(binding_id)
            );
        });
        if guarded_receipt {
            break;
        }
    }
    if !guarded_receipt {
        return None;
    }
    let domain = ctx.business_evaluation_guard.as_ref();
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
