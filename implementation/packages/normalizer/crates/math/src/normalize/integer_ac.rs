//! Canonical AC trees for proven total exact integers. Type and definedness
//! evidence precedes sorting; Add also represents Java text concatenation.
//! No source names, fixture names or rendered expressions choose rewrite rules.
use crate::{
    builder as b, expr::ExprKind, BinOpKind as B, Expr, Lit, SortContext, SymVarOrigin,
};
use num_bigint::BigInt;
use num_traits::{One, Zero};

pub(super) fn total_integer(value: &Expr) -> bool {
    total_with_temporals(value, &crate::DefaultSortContext)
}

pub(super) fn total_with_temporals(value: &Expr, ctx: &dyn SortContext) -> bool {
    match value.kind() {
        ExprKind::SymVar { origin: SymVarOrigin::Temporal { binding_id, primitive_type } } =>
            primitive_type == "integer" && (ctx.choice_slots().is_some_and(|slots| slots.contains_key(binding_id))
                || (ctx.temporal_is_total(*binding_id)
                    && crate::canonical_order::comparison_operands_are_bound(value, ctx))),
        ExprKind::SymVar { origin: SymVarOrigin::External { binding_id, path, primitive_type } } =>
            primitive_type == "integer" && path.is_empty()
                && ctx.observation_identities().and_then(|identities| identities.get(binding_id))
                    .is_some_and(|identity| matches!(identity, crate::ObservationIdentity::EntrySnapshot(_))),
        ExprKind::Lit(Lit::Int(_) | Lit::BigInt(_))
        | ExprKind::RowValue { acquired_integer: true, .. } => true,
        ExprKind::RowValue { sql: Some(column), .. } => column.scalar_type == "number",
        ExprKind::SymVar {
            // The typed origin, not its display spelling, establishes input
            // availability. Actual E and temporal values have other origins.
            origin: SymVarOrigin::Parameter { primitive_type, .. },
        } => primitive_type == "integer",
        ExprKind::BinOp {
            op: B::Add | B::Sub | B::Mul,
            left,
            right,
        } => total_with_temporals(left, ctx) && total_with_temporals(right, ctx),
        // A numeric result annotation does not prove totality. In particular,
        // E/T and CASE values may exist only on an acquisition/completion domain.
        // Word and IEEE operations retain their own explicit operator contracts.
        _ => false,
    }
}

/// Strict numeric operators preserve the domain of both operands when swapped;
/// a guarded external result does not justify eliminating or duplicating it.
pub(super) fn exact_business_integer(value: &Expr, ctx: &dyn SortContext) -> bool {
    if value.as_lit().and_then(Lit::as_integer).is_some() {
        return true;
    }
    match value.kind() {
        ExprKind::SymVar {
            origin:
                SymVarOrigin::Parameter { primitive_type, .. }
                | SymVarOrigin::External { primitive_type, .. },
        } => primitive_type == "integer",
        ExprKind::SymVar { origin: SymVarOrigin::Temporal { binding_id, primitive_type } } =>
            primitive_type == "integer" && ctx.temporal_aliases().is_some_and(|names| names.contains_key(binding_id)),
        ExprKind::BinOp {
            op: B::Add | B::Sub | B::Mul,
            left,
            right,
        } => exact_business_integer(left, ctx) && exact_business_integer(right, ctx),
        ExprKind::RowValue { acquired_integer: true, .. } => true,
        // The current physical SQL number binding is constructed only
        // for a declared exact integer column, not an unchecked number.
        ExprKind::RowValue { sql: Some(column), .. } => column.scalar_type == "number",
        ExprKind::Call { key, .. } if crate::domains::completion::output_name(&crate::func_name(*key)) == "completion.value" =>
            crate::domains::primitive::registry::func_return_type(*key) == Some(crate::Primitive::Number),
        ExprKind::Call { .. } => crate::domains::composite::field_projection(value)
            .is_some_and(|(_, _, ty)| ty == crate::Primitive::Number),
        // Strict addition/multiplication require both operands on the
        // same live path regardless of their display order. This type
        // evidence does not authorize dropping an operand or its guard.
        ExprKind::CaseWhen { .. } => value.as_total_case_when().is_some_and(|(arms, fallback)| {
            exact_business_integer(fallback, ctx)
                && arms.iter().all(|(_, result)| exact_business_integer(result, ctx))
        }),
        ExprKind::RelationAggregate { kind: crate::RelationAggregateKind::Count, .. } => true,
        ExprKind::RelationAggregate { kind: crate::RelationAggregateKind::Sum | crate::RelationAggregateKind::FirstValue,
            select_expr: Some(selected), default_expr, .. } =>
            exact_business_integer(selected, ctx) && default_expr.as_ref().is_none_or(|value|
                value.as_lit() == Some(&Lit::Null) || exact_business_integer(value, ctx)),
        _ => false,
    }
}

pub(super) fn simplify(expr: &Expr, ctx: &dyn SortContext) -> Expr {
    let ExprKind::BinOp { op: op @ (B::Add | B::Sub | B::Mul), left, right } = expr.kind() else {
        return expr.clone();
    };
    if *op == B::Sub {
        // This strict rewrite keeps the exact same acquired base exactly
        // once. The two other values are integer literals, so no evaluation,
        // observation or completion domain can be lost or reordered.
        if let Some((B::Add, x, y)) = left.as_binop() {
            let (base, first) = if y.as_lit().and_then(Lit::as_integer).is_some() {
                (x, y)
            } else { (y, x) };
            if exact_business_integer(base, ctx) {
                if let (Some(first), Some(second)) = (
                    first.as_lit().and_then(Lit::as_integer),
                    right.as_lit().and_then(Lit::as_integer),
                ) {
                    return apply_offset(base, first - second);
                }
            }
        }
        return expr.clone();
    }
    let start = std::time::Instant::now();
    let total = total_with_temporals(expr, ctx);
    let output = simplify_admitted(expr, *op, left, right, total, ctx);
    let skip_reason = if !total {
        Some("not_total_integer")
    } else if output == *expr {
        Some("no_rewrite")
    } else {
        None
    };
    if total || expr.size() >= 16 {
        crate::profile::rule_event(
            "math.integer.ac",
            expr.size(),
            output.size(),
            start.elapsed().as_micros(),
            skip_reason,
        );
    }
    output
}

fn simplify_admitted(
    expr: &Expr, op: B, left: &Expr, right: &Expr, total: bool, ctx: &dyn SortContext,
) -> Expr {
    if !total {
        if !exact_business_integer(expr, ctx)
            || !crate::canonical_order::comparison_operands_are_bound(expr, ctx)
        {
            return expr.clone();
        }
        // A strict AC expression still requires every acquired operand.
        // Canonicalize the whole chain but never erase an observation,
        // even if a multiplication by zero would hide its unavailability.
        fn strict_leaves(value: &Expr, op: B, out: &mut Vec<Expr>) {
            if let Some((inner, left, right)) = value.as_binop() {
                if inner == op {
                    strict_leaves(left, op, out);
                    strict_leaves(right, op, out);
                    return;
                }
            }
            out.push(value.clone());
        }
        let mut leaves = Vec::new();
        strict_leaves(expr, op, &mut leaves);
        leaves.sort_by_cached_key(|value| crate::canonical_order_key(value, ctx));
        return leaves.into_iter().reduce(|left, right| b::binop_public(op, left, right))
            .expect("binary exact arithmetic has operands");
    }
    if op == B::Add {
        if let Some(offset) = combine_constant_offset(left, right)
            .or_else(|| combine_constant_offset(right, left))
        {
            return offset;
        }
    }
    fn collect(expr: &Expr, op: B, out: &mut Vec<Expr>) {
        if let ExprKind::BinOp {
            op: inner,
            left,
            right,
        } = expr.kind()
        {
            if *inner == op {
                collect(left, op, out);
                collect(right, op, out);
                return;
            }
        }
        out.push(expr.clone());
    }
    let mut leaves = Vec::new();
    collect(expr, op, &mut leaves);
    let mut constant = if op == B::Add {
        BigInt::zero()
    } else {
        BigInt::one()
    };
    let mut terms = Vec::new();
    for leaf in leaves {
        if let Some(value) = leaf.as_lit().and_then(Lit::as_integer) {
            if op == B::Add {
                constant += value;
            } else {
                constant *= value;
            }
        } else {
            terms.push(leaf);
        }
    }
    // Only the total exact fragment reaches here. A zero factor can
    // absorb already acquired values, but not a partial observation.
    if op == B::Mul && constant.is_zero() {
        return b::lit_int(0);
    }
    let identity = if op == B::Add {
        constant.is_zero()
    } else {
        constant.is_one()
    };
    if !identity || terms.is_empty() {
        terms.push(b::lit_integer(constant));
    }
    // This shared structural comparator is legal only after the proof above.
    terms.sort_by(|left, right| crate::canonical_order_compare(left, right, ctx));
    let combine: fn(Expr, Expr) -> Expr = if op == B::Add { b::add } else { b::mul };
    terms.into_iter().reduce(combine).unwrap()
}

// A proven total exact integer base is evaluated once. Combine only the two
// literal offsets; do not commute subtraction or expand a symbolic sign.
fn combine_constant_offset(difference: &Expr, addition: &Expr) -> Option<Expr> {
    let (B::Sub, base, subtraction) = difference.as_binop()? else { return None; };
    let offset = addition.as_lit()?.as_integer()? - subtraction.as_lit()?.as_integer()?;
    Some(apply_offset(base, offset))
}

fn apply_offset(base: &Expr, offset: BigInt) -> Expr {
    match offset.cmp(&BigInt::zero()) {
        std::cmp::Ordering::Equal => base.clone(),
        std::cmp::Ordering::Less => b::sub(base.clone(), b::lit_integer(-offset)),
        std::cmp::Ordering::Greater => b::add(base.clone(), b::lit_integer(offset)),
    }
}
