//! Ordered, total exact-integer folds retained by the source evaluator.
use super::*;
use normalizer_ir::local::relation::{Op, Scalar};

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Definition {
    pub(super) source: String,
    pub(super) initial: Expr,
    pub(super) body: Expr,
}

fn direct_capture_input(engine: &Engine<'_>, expr: &Expr, primitive: &str) -> bool {
    matches!(expr.as_sym_var(), Some(SymVarOrigin::Parameter { name, primitive_type })
        if primitive_type == primitive && !matches!(name.as_str(), "acc" | "item") && !name.contains(['.', '[', ']'])
            && engine.declared_relation_input(name, primitive))
}

fn closed_integer_capture(engine: &Engine<'_>, expr: &Expr) -> bool {
    if expr.as_lit().and_then(math::Lit::as_integer).is_some() || direct_capture_input(engine, expr, "integer") {
        return true;
    }
    if let Some((math::UnaryOpKind::Neg, operand)) = expr.as_unary_op() {
        return closed_integer_capture(engine, operand);
    }
    if let Some(([(guard, then_value)], fallback)) = expr.as_total_case_when() {
        return direct_capture_input(engine, guard, "boolean")
            && closed_integer_capture(engine, then_value) && closed_integer_capture(engine, fallback);
    }
    expr.as_binop().is_some_and(|(op, left, right)| {
        matches!(op, BinOpKind::Add | BinOpKind::Sub | BinOpKind::Mul)
            && closed_integer_capture(engine, left) && closed_integer_capture(engine, right)
    })
}

fn local_body(engine: &Engine<'_>, expr: &Scalar, acc: &str, item: &str, env: &Env) -> Result<Expr> {
    Ok(match expr {
        Scalar::Path(parts) if parts.len() == 1 && parts[0] == acc => b::param("acc", ScalarSymbolType::Integer),
        Scalar::Path(parts) if parts.len() == 1 && parts[0] == item => b::param("item", ScalarSymbolType::Integer),
        Scalar::Path(parts) if parts.len() == 1 => {
            let value = env.get(&parts[0]).filter(|value| value.ty() == Ty::Integer)
                .ok_or_else(|| fail("pure fold body requires closed exact-integer arithmetic; capture is not an acquired exact integer"))?;
            let expression = value.scalar()?;
            need(expression.size() <= 2048 && expression.depth() <= 48
                && closed_integer_capture(engine, &expression),
                "pure fold body requires closed exact-integer arithmetic; capture is not over acquired direct inputs")?;
            expression
        }
        Scalar::Integer(n) => b::lit_integer(n.clone()),
        Scalar::Binary(op @ (Op::Add | Op::Sub | Op::Mul), left, right) => {
            let build = super::business_relation::scalar_binary(*op);
            build(local_body(engine, left, acc, item, env)?, local_body(engine, right, acc, item, env)?)
        }
        _ => return Err(fail("pure fold body requires closed exact-integer arithmetic")),
    })
}

impl Engine<'_> {
    pub(super) fn retain_pure_fold(
        &mut self, source: &str, acc: &str, item: &str,
        initial: Expr, body: &Scalar, guard: &Expr, env: &Env,
    ) -> Result<Value> {
        need(guard.as_bool() == Some(true) && acc != item,
            "pure fold requires unconditional evaluation over one ordered source array")?;
        let definition = Definition {
            source: source.into(),
            initial,
            body: math::normalize(local_body(self, body, acc, item, env)?),
        };
        let node = super::temporal_graph::Definition::Fold(definition);
        let id = self.temporal_definitions.iter().position(|old| old == &node).unwrap_or_else(|| {
            let id = self.temporal_definitions.len();
            self.temporal_definitions.push(node);
            id
        }) as u64;
        Ok(Value::Term(Ty::Integer, b::integer_temporal_result(id)))
    }
}
