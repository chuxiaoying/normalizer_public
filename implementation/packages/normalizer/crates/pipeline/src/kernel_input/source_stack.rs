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
        need(self.selected_entry == Some(method), "local stack requires typed entry arrays")?;
        need(exception == "IllegalArgumentException", "local stack uses the same supported type-only exception for both guards")?;
        let root_of = |local: &str| -> Result<(String, &str)> {
            let Some(Value::Sequence { item: Ty::Ref, name, .. }) = env.get(local)
                else { return Err(fail("local stack requires bound record arrays")); };
            let owner = self.source_relations.record_inputs.get(name)
                .ok_or_else(|| fail("local stack array needs a declared entry origin"))?;
            need(!self.source_array_writes.contains_key(name), "local stack arrays are read-only acquired inputs")?;
            Ok((name.clone(), owner))
        };
        let (opens, opened_owner) = root_of(opens)?;
        let (closes, closed_owner) = root_of(closes)?;
        let declared = self.program.find_class(opened_owner)
            .ok_or_else(|| fail("local stack has no record element declaration"))?;
        need(opened_owner == closed_owner && declared.fields().iter().any(|f|
            !f.is_static() && f.name() == field && crate::normalized::integral(f.java_type())),
            "local stack requires one declared integer field in the same input record type")?;
        let (empty, failed) = math::domains::relation::positional::reverse_stack_completion(&opens, &closes, field);
        self.throw_source_on_path(b::and(path, failed), "java.lang.IllegalArgumentException");
        env.insert(binding.clone(), Value::Term(Ty::Bool, math::normalize(empty)));
        Ok(4)
    }
}
