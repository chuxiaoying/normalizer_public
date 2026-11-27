//! 常量折叠规则。

use crate::builder;
use crate::expr::{BinOpKind, Expr, ExprKind, Lit, UnaryOpKind};

/// 常量折叠和已认证 Vector 的自然域边界事实。
pub(crate) fn fold(expr: &Expr) -> Expr {
    if let Some(length) = crate::domains::composite::fold_vector_length_predicate(expr) {
        return length;
    }
    let folded = match expr.kind() {
        ExprKind::BinOp { op, left, right } => left.as_lit().zip(right.as_lit())
            .and_then(|(a, b)| fold_binop(*op, a, b)),
        ExprKind::UnaryOp { op, operand } => operand.as_lit()
            .and_then(|value| fold_unary(*op, value)),
        ExprKind::Call { key, args } => fold_call(*key, args),
        _ => None,
    };
    // An inapplicable literal rule is not a second admission check. All
    // non-applicable operators, including symbolic forms, keep the same node
    // for the other normalization rules to inspect.
    folded.unwrap_or_else(|| expr.clone())
}

/// Fold calls whose pure registry contract provides a result. An actual
/// null trigger uses the same declaration as the symbolic Boolean rules.
/// Ordinary evaluators need literal operands and run before symbolic rules.
fn fold_call(
    func_key: crate::domains::primitive::registry::FuncKey,
    args: &[Expr],
) -> Option<Expr> {
    let sem = crate::domains::primitive::registry::func_semantics(func_key);
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
    crate::domains::text::repeat_trim(func_key, args)
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
    use Lit::*;
    use UnaryOpKind::*;
    match (op, v) {
        (Neg, Int(n)) => Some(builder::lit_integer(-num_bigint::BigInt::from(*n))),
        (Neg, BigInt(n)) => Some(builder::lit_integer(-n.clone())),
        (Not, Bool(x)) => Some(builder::lit_bool(!x)),
        _ => None,
    }
}
