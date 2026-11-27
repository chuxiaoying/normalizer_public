//! Symbolic execution for a certified local push-then-pop fold.
use super::*;
use normalizer_ir::local::{stack::Action, LocalIr};

impl Engine<'_> {
    pub(super) fn execute_closed_source_stack(
        &mut self, statements: &[S], method: &str, env: &mut Env, path: Expr,
    ) -> Result<usize> {
        let [
            S::LocalIr(LocalIr::Stack(Action::PushAll { stack, source: opens })),
            S::LocalIr(LocalIr::Stack(Action::CheckAndPop { stack: popped, source: closes, field, exception })),
            S::VarDecl { java_type, name, initializer: None, .. },
            S::LocalIr(LocalIr::Stack(Action::IsEmpty { stack: tested, binding })),
            ..
        ] = statements else {
            return Err(fail("local stack requires contiguous push, guarded pop and typed emptiness observation"));
        };
        need(
            stack == popped && stack == tested && name == binding
                && !env.contains_key(stack) && !env.contains_key(binding)
                && self.project.declared_result_type(java_type)? == Ty::Bool,
            "local stack or its output is not a fresh closed binding",
        )?;
        need(self.selected_entry.id == method, "local stack requires typed entry arrays")?;
        let exception = self.project.source_exception_bindings.get(method)
            .and_then(|bindings| bindings.get(exception))
            .filter(|name| self.project.type_only_exception(name))
            .ok_or_else(|| fail("local stack requires its resolved type-only exception"))?
            .clone();
        let root_of = |local: &str| -> Result<(String, &str)> {
            let Some(Value::Sequence { item: Ty::Ref, name, .. }) = env.get(local)
                else { return Err(fail("local stack requires bound record arrays")); };
            let owner = self.source_relations.record_inputs.get(name)
                .ok_or_else(|| fail("local stack array needs a declared entry origin"))?;
            need(!self.source_array_may_have_writes(name), "local stack arrays are read-only acquired inputs")?;
            Ok((name.clone(), owner))
        };
        let (opens, opened_owner) = root_of(opens)?;
        let (closes, closed_owner) = root_of(closes)?;
        let match_type = |owner| -> Option<math::Primitive> {
            let declared = self.program.find_class(owner)?;
            let field = declared.fields().iter().find(|candidate| candidate.name() == field && !candidate.is_static())?;
            match field.java_type() {
                ty if crate::normalized::integral(ty) => Some(math::Primitive::Number),
                JavaType::Primitive(name) if name == "boolean" => Some(math::Primitive::Boolean),
                _ => None,
            }
        };
        let ty = match_type(opened_owner).filter(|opened| Some(*opened) == match_type(closed_owner))
            .ok_or_else(|| fail("local stack requires the same declared integer or Boolean field contract in both record types"))?;
        let (empty, failed) = math::domains::relation::positional::reverse_stack_completion(&opens, &closes, field, ty);
        self.throw_source_on_path(b::and(path, failed), &exception);
        env.insert(binding.clone(), Value::Term(Ty::Bool, math::normalize(empty)));
        Ok(4)
    }
}
