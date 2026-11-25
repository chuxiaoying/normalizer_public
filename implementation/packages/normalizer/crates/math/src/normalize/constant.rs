//! 常量折叠规则。

use crate::builder;
use crate::expr::{BinOpKind, Expr, ExprKind, Lit, UnaryOpKind};

/// 常量折叠：对运算符的所有子式都是 Lit 的情况求值。
pub(crate) fn fold(expr: &Expr) -> Expr {
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

/// Call 节点折叠：pure_function + all-Lit args + evaluator Some 时求值。
///
/// 三条件全满足：
/// 1. `func_semantics(func_key).pure == true`（声明为纯函数）
/// 2. `func_semantics(func_key).evaluator.is_some()`（提供 evaluator）
/// 3. 所有 args 都是 `ExprKind::Lit`（参数全为常量）
///
/// evaluator 返回：
/// - `Some(lit)`：折叠为该 Lit
/// - `None`：evaluator 不支持此输入组合（如类型不匹配），保留原 Call
///
/// P2 方案 B 核心实现。
fn fold_call(
    func_key: crate::domains::primitive::registry::FuncKey,
    args: &[Expr],
) -> Option<Expr> {
    if let Some(value) = crate::domains::completion::reduce(func_key, args) {
        return Some(value);
    }
    if let Some(value) = crate::domains::composite::reduce(func_key, args) {
        return Some(value);
    }
    let sem = crate::domains::primitive::registry::func_semantics(func_key);
    if !sem.pure {
        return None;
    }
    if let Some(reducer) = &sem.symbolic_reducer {
        if let Some(value) = symbolic_equation(func_key, args, reducer) {
            return Some(value);
        }
    }
    let evaluator = sem.evaluator.as_ref()?;
    let mut lits: Vec<Lit> = Vec::with_capacity(args.len());
    for a in args.iter() {
        match a.kind() {
            ExprKind::Lit(l) => lits.push(l.clone()),
            _ => return None,
        }
    }
    evaluator(&lits).map(builder::lit)
}

thread_local! {
    static ACTIVE_SYMBOLIC: std::cell::RefCell<std::collections::HashSet<Expr>> = Default::default();
}
struct SymbolicGuard(Expr);
impl Drop for SymbolicGuard {
    fn drop(&mut self) {
        ACTIVE_SYMBOLIC.with(|active| {
            active.borrow_mut().remove(&self.0);
        });
    }
}
/// Multiset extension of strict natural-number descent. Only calls with a
/// registered symbolic equation contribute; boolean guards may grow while
/// blank(concat(a,b)) becomes blank(a), blank(b) on strictly smaller arguments.
fn symbolic_rank(value: &Expr) -> Vec<usize> {
    let mut rank: Vec<usize> = vec![];
    value.walk(&mut |part| {
        if let Some((key, args)) = part.as_call() {
            let semantics = crate::domains::primitive::registry::func_semantics(key);
            if semantics.pure && semantics.symbolic_reducer.is_some() {
                rank.push(args.iter().map(Expr::size).sum::<usize>());
            }
        }
    });
    rank.sort_unstable_by(|a, b| b.cmp(a));
    rank
}
fn symbolic_equation(
    key: crate::FuncKey,
    args: &[Expr],
    reducer: &std::sync::Arc<dyn Fn(crate::FuncKey, &[Expr]) -> Option<Expr> + Send + Sync>,
) -> Option<Expr> {
    assert_eq!(
        args.len(),
        crate::domains::primitive::registry::func_arity(key),
        "[MATH-SYMBOLIC-CONTRACT] argument arity changed"
    );
    // Boolean and nullable text have complete scalar result sorts. Number
    // does not encode source word width, and composite/ref equations need a
    // separate type-preserving contract before admission.
    let result_sort = crate::domains::primitive::registry::func_return_type(key);
    assert!(
        matches!(
            result_sort,
            Some(crate::Primitive::Boolean | crate::Primitive::String)
        ),
        "[MATH-SYMBOLIC-TYPE] symbolic equations require Boolean or String return type"
    );
    let original = builder::call(key, args.to_vec());
    let entered = ACTIVE_SYMBOLIC.with(|active| active.borrow_mut().insert(original.clone()));
    assert!(entered, "[MATH-SYMBOLIC-CYCLE] reentrant symbolic equation");
    let _guard = SymbolicGuard(original.clone());
    let result = reducer(key, args)?;
    if result == original {
        return None;
    }
    assert!(
        symbolic_rank(&result) < symbolic_rank(&original),
        "[MATH-SYMBOLIC-NONDECREASING] symbolic equation must strictly decrease its call rank"
    );
    match result_sort {
        Some(crate::Primitive::Boolean) => assert!(
            super::boolean_casewhen::is_boolean_expr(&result),
            "[MATH-SYMBOLIC-TYPE] predicate equation returned a non-boolean expression"
        ),
        Some(crate::Primitive::String) => assert!(
            is_symbolic_text(&result),
            "[MATH-SYMBOLIC-TYPE] text equation returned an untyped or non-text expression"
        ),
        _ => unreachable!(),
    }
    Some(result)
}

// Do not infer a text sort for undeclared row, relation, or structural values.
fn is_symbolic_text(value: &Expr) -> bool {
    match value.kind() {
        ExprKind::Lit(Lit::Str(_) | Lit::Null) => true,
        ExprKind::SymVar { origin } => {
            let ty = match origin {
                crate::SymVarOrigin::Parameter { primitive_type, .. }
                | crate::SymVarOrigin::Temporal { primitive_type, .. }
                | crate::SymVarOrigin::External { primitive_type, .. } => primitive_type,
            };
            ty == "java.lang.String"
        }
        ExprKind::Call { key, .. } =>
            crate::domains::primitive::registry::func_return_type(*key)
                    == Some(crate::Primitive::String),
        ExprKind::CaseWhen { branches } =>
            branches.last().is_some_and(|(c, _)| c.as_bool() == Some(true))
                && branches.iter().all(|(_, value)| is_symbolic_text(value)),
        _ => false,
    }
}

fn fold_binop(op: BinOpKind, a: &Lit, b: &Lit) -> Option<Expr> {
    use BinOpKind::*;
    use Lit::*;
    if let (Some(x), Some(y)) = (a.as_integer(), b.as_integer()) {
        return match op {
            Add => Some(builder::lit_integer(x + y)),
            Sub => Some(builder::lit_integer(x - y)),
            Mul => Some(builder::lit_integer(x * y)),
            Eq => Some(builder::lit_bool(x == y)),
            Ne => Some(builder::lit_bool(x != y)),
            Lt => Some(builder::lit_bool(x < y)),
            Le => Some(builder::lit_bool(x <= y)),
            Gt => Some(builder::lit_bool(x > y)),
            Ge => Some(builder::lit_bool(x >= y)),
            _ => None,
        };
    }
    match (op, a, b) {
        // String literal concat: 纯数学折叠，两侧均为 Str(Lit) 时直接拼接。
        // 归一化纪律："ok: " + "default" → "ok: default" 不应保留为 BinOp。
        (Add, Str(x), Str(y)) => Some(builder::lit_str(format!("{}{}", x, y))),

        (Eq, Str(x), Str(y)) => Some(builder::lit_bool(x == y)),
        (Ne, Str(x), Str(y)) => Some(builder::lit_bool(x != y)),
        (Eq, Bool(x), Bool(y)) => Some(builder::lit_bool(x == y)),
        (Ne, Bool(x), Bool(y)) => Some(builder::lit_bool(x != y)),
        (Eq, Null, Null) => Some(builder::lit_bool(true)),
        (Ne, Null, Null) => Some(builder::lit_bool(false)),
        (Eq, Null, Str(_) | Int(_) | BigInt(_) | Bool(_)) => Some(builder::lit_bool(false)),
        (Eq, Str(_) | Int(_) | BigInt(_) | Bool(_), Null) => Some(builder::lit_bool(false)),
        (Ne, Null, Str(_) | Int(_) | BigInt(_) | Bool(_)) => Some(builder::lit_bool(true)),
        (Ne, Str(_) | Int(_) | BigInt(_) | Bool(_), Null) => Some(builder::lit_bool(true)),

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
