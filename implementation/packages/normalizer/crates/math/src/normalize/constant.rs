//! 常量折叠规则。

use crate::builder;
use crate::expr::{BinOpKind, Expr, ExprKind, Lit, UnaryOpKind};

/// 常量折叠和已认证 Vector 的自然域边界事实。
pub(crate) fn fold(expr: &Expr) -> Expr {
    if let Some(length) = crate::domains::composite::fold_vector_length_predicate(expr) {
        return length;
    }
    match expr.kind() {
        ExprKind::BinOp { op, left, right } => {
            if let (ExprKind::Lit(a), ExprKind::Lit(b)) = (left.kind(), right.kind()) {
                if let Some(folded) = fold_binop(*op, a, b) {
                    return folded;
                }
            }
            expr.clone()
        }

        ExprKind::UnaryOp { op, operand } => {
            if let ExprKind::Lit(v) = operand.kind() {
                if let Some(folded) = fold_unary(*op, v) {
                    return folded;
                }
            }
            expr.clone()
        }

        ExprKind::Call {
            key,
            args,
        } => fold_call(*key, args).unwrap_or_else(|| expr.clone()),

        _ => expr.clone(),
    }
}

/// Fold calls whose pure registry contract provides a result. An actual
/// null trigger uses the same declaration as the symbolic Boolean rules.
/// Ordinary evaluators need literal operands and run before symbolic rules.
fn fold_call(
    func_key: crate::domains::primitive::registry::FuncKey,
    args: &[Expr],
) -> Option<Expr> {
    if let Some(value) = crate::domains::composite::reduce(func_key, args) {
        return Some(value);
    }
    let sem = crate::domains::primitive::registry::func_semantics(func_key);
    if !sem.pure {
        return None;
    }
    if let Some(null_behavior) = &sem.null_behavior {
        if null_behavior.trigger_positions.iter().any(|&position| {
            args.get(position).is_some_and(Expr::is_null)
        }) {
            return Some(builder::lit(null_behavior.result.clone()));
        }
    }
    if let Some(evaluator) = &sem.evaluator {
        let literals = args.iter().map(|arg| arg.as_lit().cloned()).collect::<Option<Vec<_>>>();
        if let Some(value) = literals.and_then(|values| evaluator(&values)) {
            return Some(builder::lit(value));
        }
    }
    // Return a direct child of the same registered unary Call, so the
    // rewrite preserves the result sort and strictly decreases node count.
    if sem.unary_idempotent {
        if let [value] = args {
            if value.as_call().is_some_and(|(inner, _)| inner == func_key) {
                return Some(value.clone());
            }
        }
    }
    None
}

fn fold_binop(op: BinOpKind, a: &Lit, b: &Lit) -> Option<Expr> {
    use BinOpKind::*;
    use Lit::*;
    if let (Some(x), Some(y)) = (a.as_integer(), b.as_integer()) {
        return match op {
            Add => Some(builder::lit_integer(x + y)),
            Sub => Some(builder::lit_integer(x - y)),
            Mul => Some(builder::lit_integer(x * y)),
            _ => op.comparison_test().map(|compare| builder::lit_bool(compare(&x, &y))),
        };
    }
    match (op, a, b) {
        // String literal concat: 纯数学折叠，两侧均为 Str(Lit) 时直接拼接。
        // 归一化纪律："ok: " + "default" → "ok: default" 不应保留为 BinOp。
        (Add, Str(x), Str(y)) => Some(builder::lit_str(format!("{}{}", x, y))),

        (Eq | Ne, Null, _) | (Eq | Ne, _, Null)
        | (Eq | Ne, Str(_), Str(_)) | (Eq | Ne, Bool(_), Bool(_)) =>
            Some(builder::lit_bool((a == b) == (op == Eq))),

        (And, Bool(x), Bool(y)) => Some(builder::lit_bool(*x && *y)),
        (Or, Bool(x), Bool(y)) => Some(builder::lit_bool(*x || *y)),

        _ => None,
    }
}

fn fold_unary(op: UnaryOpKind, v: &Lit) -> Option<Expr> {
    if op == UnaryOpKind::Neg {
        if let Some(value) = v.as_integer() {
            return Some(builder::lit_integer(-value));
        }
    }
    use Lit::*;
    use UnaryOpKind::*;
    match (op, v) {
        (Not, Bool(x)) => Some(builder::lit_bool(!x)),
        _ => None,
    }
}
