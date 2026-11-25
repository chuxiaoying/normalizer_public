//! Ordered, total exact-integer folds retained by the source evaluator.
use super::*;
use normalizer_ir::local::relation::{Op, Scalar};

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Definition {
    pub(super) source: String,
    pub(super) initial: Expr,
    pub(super) body: Expr,
}

fn local_body(expr: &Scalar, acc: &str, item: &str) -> Result<Expr> {
    Ok(match expr {
        Scalar::Path(parts) if parts.len() == 1 && parts[0] == acc => b::param("acc", "integer"),
        Scalar::Path(parts) if parts.len() == 1 && parts[0] == item => b::param("item", "integer"),
        Scalar::Literal(normalizer_ir::local::ScalarOperand::Integer(n)) => b::lit_integer(n.clone()),
        Scalar::Binary(op @ (Op::Add | Op::Sub | Op::Mul), left, right) => {
            let build = match op { Op::Add => b::add, Op::Sub => b::sub, _ => b::mul };
            build(local_body(left, acc, item)?, local_body(right, acc, item)?)
        }
        _ => return Err(fail("pure fold body requires closed exact-integer arithmetic")),
    })
}

impl Engine<'_> {
    pub(super) fn retain_pure_fold(
        &mut self, source: &str, acc: &str, item: &str,
        initial: Expr, body: &Scalar, guard: &Expr,
    ) -> Result<Value> {
        need(guard.as_bool() == Some(true) && acc != item,
            "pure fold requires unconditional evaluation over one ordered source array")?;
        let definition = Definition {
            source: source.into(),
            initial,
            body: math::normalize(local_body(body, acc, item)?),
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
