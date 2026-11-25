//! Typed normal and exceptional views of the same external occurrence.
use super::Ty;
use math::{builder as b, domains::completion, Expr};
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub(crate) struct PortSpec {
    pub returns: Ty,
    pub exceptions: BTreeMap<String, Ty>,
}
impl PortSpec {
    pub fn throwing(&self) -> bool {
        !self.exceptions.is_empty()
    }
    pub fn normal_condition(&self, response: &Expr) -> Expr {
        math::normalize(b::not(
            self.exceptions
                .keys()
                .fold(b::lit_bool(false), |condition, class| {
                    b::or(condition, completion::is_throw(response.clone(), class))
                }),
        ))
    }
    /// Await observes a tagged completion even for a callback declaring no rejection.
    pub(super) fn awaited_value(&self, response: &Expr) -> super::Result<Expr> {
        if self.returns == Ty::Unit {
            return Ok(b::lit_null());
        }
        Ok(completion::value(
            response.clone(),
            self.returns.primitive_value()?,
        ))
    }
    pub fn normal_value(&self, response: &Expr) -> Expr {
        if self.returns == Ty::Unit {
            return b::lit_null();
        }
        if !self.throwing() {
            return response.clone();
        }
        completion::value(
            response.clone(),
            self.returns
                .primitive_value()
                .expect("certified closed response type"),
        )
    }
    pub fn throw_variants(&self, response: &Expr) -> Vec<(String, Expr, Expr)> {
        self.exceptions
            .iter()
            .map(|(class, ty)| {
                let payload = if *ty == Ty::Unit {
                    b::lit_null()
                } else {
                    completion::payload(
                        response.clone(),
                        class,
                        ty.primitive_value().expect("certified closed payload type"),
                    )
                };
                (
                    class.clone(),
                    completion::is_throw(response.clone(), class),
                    payload,
                )
            })
            .collect()
    }
}
