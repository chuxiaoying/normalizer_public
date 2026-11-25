//! Literal results declared for null inputs to registered operations.

use crate::builder;
use crate::domains::primitive::registry::func_semantics;
use crate::expr::{Expr, ExprKind, Lit};

pub(crate) fn simplify(expr: &Expr) -> Expr {
    if let ExprKind::Call { key, args, .. } = expr.kind() {
        let semantics = func_semantics(*key);

        // Null 行为
        if let Some(null_beh) = &semantics.null_behavior {
            let triggered = null_beh.trigger_positions.iter().any(|&pos| {
                pos < args.len() && matches!(args[pos].kind(), ExprKind::Lit(Lit::Null))
            });
            if triggered {
                return match &null_beh.result {
                    Lit::Int(n) => builder::lit_int(*n),
                    Lit::BigInt(n) => builder::lit_integer(n.clone()),
                    Lit::Str(s) => builder::lit_str(s.clone()),
                    Lit::Bool(b) => builder::lit_bool(*b),
                    Lit::Null => builder::lit_null(),
                };
            }
        }
    }
    expr.clone()
}
