//! Typed lowering of explicitly declared exact decimal source operations.
use super::*;
use math::domains::decimal as number;
use normalizer_ir::local::decimal::{Assignment, Decimal, Mode};

fn decimal(expr: &Decimal, env: &Env) -> Result<Expr> {
    match expr {
        Decimal::Local(name) => {
            let value = env
                .get(name)
                .ok_or_else(|| fail("decimal operand is not bound"))?;
            need(
                value.ty() == Ty::Decimal,
                "decimal operand must have the exact decimal carrier",
            )?;
            value.scalar()
        }
        Decimal::Literal { coefficient, scale } => {
            let scale = u32::try_from(*scale).map_err(|_| {
                "[NORM-ANALYSIS-INCOMPLETE] exact decimal literal exceeds the scale representation"
            })?;
            Ok(number::literal(coefficient.clone(), scale))
        }
        Decimal::Product(factors) => {
            if factors.len() > 32 {
                return Err(
                    "[NORM-ANALYSIS-INCOMPLETE] decimal product exceeds its factor budget".into(),
                );
            }
            let mut factors = factors.iter().map(|value| decimal(value, env));
            let initial = factors
                .next()
                .ok_or_else(|| fail("decimal product requires at least one factor"))??;
            factors.try_fold(initial, |product, value| {
                Ok(number::multiply(product, value?))
            })
        }
    }
}

impl Engine<'_> {
    pub(super) fn execute_source_decimal(
        &self,
        op: &Assignment,
        declare: &JavaType,
        env: &mut Env,
    ) -> Result<()> {
        need(
            crate::normalized::exact_decimal_type(declare),
            "decimal IR requires a declared java.math.BigDecimal local",
        )?;
        let value = decimal(&op.value, env)?;
        let value = match &op.round {
            None => value,
            Some((scale, source)) => {
                let mode = match source {
                    Mode::HalfUp => number::Mode::HalfUp,
                    Mode::HalfEven => number::Mode::HalfEven,
                };
                number::round_at(value, b::lit_integer(scale.clone()), mode)
            }
        };
        env.insert(op.binding.clone(), Value::Term(Ty::Decimal, value));
        Ok(())
    }
}
