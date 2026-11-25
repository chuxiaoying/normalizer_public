//! Canonical AC trees for proven total exact integers. Type and definedness
//! evidence precedes sorting; Add also represents Java text concatenation.
//! No source names, fixture names or rendered expressions choose rewrite rules.
use crate::{
    builder as b, expr::ExprKind, BinOpKind as B, Expr, Lit, SortContext, SymVarOrigin, UnaryOpKind,
};
use num_bigint::BigInt;
use num_traits::{One, Zero};

pub(super) fn total_integer(value: &Expr) -> bool {
    match value.kind() {
        ExprKind::Lit(Lit::Int(_) | Lit::BigInt(_)) => true,
        ExprKind::SymVar {
            origin:
                SymVarOrigin::Parameter {
                    name,
                    primitive_type,
                },
        } =>
        // Legacy field expansion can encode partial E/heap projections as
        // Parameter paths. Only a direct entry parameter is evidence here;
        // a numeric annotation alone does not establish a field read domain.
        {
            !name.starts_with("$e_")
                && !name.starts_with("$t_")
                && !name.starts_with("$h_")
                && !name.contains(['[', ']'])
                && (primitive_type == "integer" || !name.contains('.'))
                && matches!(
                    primitive_type.as_str(),
                    "integer" | "byte" | "short" | "int" | "long" | "char"
                )
        }
        ExprKind::BinOp {
            op: B::Add | B::Sub | B::Mul,
            left,
            right,
        } => total_integer(left) && total_integer(right),
        ExprKind::UnaryOp {
            op: UnaryOpKind::Neg,
            operand,
        } => total_integer(operand),
        // A numeric result annotation does not prove totality. In particular,
        // E/T and CASE values may exist only on an acquisition/completion domain.
        // Word and IEEE operations retain their own explicit operator contracts.
        _ => false,
    }
}

/// Strict numeric operators preserve the domain of both operands when swapped;
/// a guarded external result does not justify eliminating or duplicating it.
fn exact_business_integer(value: &Expr) -> bool {
    if value.as_lit().and_then(Lit::as_integer).is_some() {
        return true;
    }
    match value.kind() {
        ExprKind::SymVar {
            origin:
                SymVarOrigin::Parameter { primitive_type, .. }
                | SymVarOrigin::External { primitive_type, .. },
        } => primitive_type == "integer",
        ExprKind::BinOp {
            op: B::Add | B::Sub | B::Mul,
            left,
            right,
        } => exact_business_integer(left) && exact_business_integer(right),
        ExprKind::UnaryOp {
            op: UnaryOpKind::Neg,
            operand,
        } => exact_business_integer(operand),
        _ => false,
    }
}

pub(super) fn simplify(expr: &Expr, ctx: &dyn SortContext) -> Expr {
    let start = std::time::Instant::now();
    let (eligible_shape, total) = match expr.kind() {
        ExprKind::BinOp { op, .. } if matches!(op, B::Add | B::Mul) => (true, total_integer(expr)),
        _ => (false, false),
    };
    let output = simplify_impl(expr, ctx);
    let skip_reason = if !eligible_shape {
        Some("not_add_mul")
    } else if !total {
        Some("not_total_integer")
    } else if output == *expr {
        Some("no_rewrite")
    } else {
        None
    };
    if eligible_shape && (total || expr.size() >= 16) {
        crate::profile::rule_event(
            "math.integer.ac",
            expr.size(),
            output.size(),
            start.elapsed().as_micros(),
            skip_reason,
            &[
                ("candidate_nodes", expr.size().to_string()),
                ("rewrite", (output != *expr).to_string()),
            ],
        );
    }
    output
}

fn simplify_impl(expr: &Expr, ctx: &dyn SortContext) -> Expr {
    let ExprKind::BinOp { op, left, right } = expr.kind() else {
        return expr.clone();
    };
    if !matches!(op, B::Add | B::Mul) {
        return expr.clone();
    }
    if !total_integer(expr) {
        if exact_business_integer(expr) && crate::ordering_axiom_compare(left, right, ctx).is_gt() {
            return b::binop_public(*op, right.clone(), left.clone());
        }
        return expr.clone();
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
    collect(expr, *op, &mut leaves);
    let mut constant = if *op == B::Add {
        BigInt::zero()
    } else {
        BigInt::one()
    };
    let mut terms = Vec::new();
    for leaf in leaves {
        if let Some(value) = leaf.as_lit().and_then(Lit::as_integer) {
            if *op == B::Add {
                constant += value;
            } else {
                constant *= value;
            }
        } else {
            terms.push(leaf);
        }
    }
    if *op == B::Mul && constant.is_zero() {
        return b::lit_int(0);
    }
    let identity = if *op == B::Add {
        constant.is_zero()
    } else {
        constant.is_one()
    };
    if !identity || terms.is_empty() {
        terms.push(b::lit_integer(constant));
    }
    // This shared structural comparator is legal only after the proof above.
    terms.sort_by(|left, right| crate::ordering_axiom_compare(left, right, ctx));
    terms
        .into_iter()
        .reduce(|left, right| match op {
            B::Add => b::add(left, right),
            B::Mul => b::mul(left, right),
            _ => unreachable!(),
        })
        .unwrap()
}
