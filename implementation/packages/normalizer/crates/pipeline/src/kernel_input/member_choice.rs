//! Independent source-owned choices. Unlike a pure fold, every occurrence has
//! an identity and stays in execution order even when candidate sets agree.
use super::*;
use normalizer_ir::local::ScalarOperand;
use serde_json::json;

#[derive(Clone)]
pub(super) struct Occurrence {
    members: String,
    empty: Expr,
}

impl Engine<'_> {
    pub(super) fn choose_source_member(
        &mut self, binding: &str, members: &str, empty: &ScalarOperand,
        method: &str, env: &mut Env, path: Expr,
    ) -> Result<()> {
        need(path.as_bool() == Some(true), "conditional member choice needs a guarded T contract")?;
        let Some(Value::Sequence {item: Ty::Integer, relation: RelationExpr::Base {name, ..}, ..}) =
            env.get(members) else { return Err(fail("member choice requires a direct integer source array")); };
        need(self.methods[method].params().iter().any(|(ty, param)|
            param == name && crate::normalized::numeric_array(ty)),
            "member choice requires an acquired entry array")?;
        let empty = match empty {
            ScalarOperand::Integer(value) => b::lit_integer(value.clone()),
            ScalarOperand::Local(name) => {
                let value = env.get(name).ok_or_else(|| fail("unbound member choice empty fallback"))?;
                need(value.ty() == Ty::Integer, "member choice empty fallback must be an exact integer")?;
                value.scalar()?
            }
            ScalarOperand::Text(_) => return Err(fail("member choice empty fallback must be an exact integer")),
        };
        let occurrence = Occurrence {members: name.clone(), empty};
        let id = self.member_choices.len();
        self.member_choices.push(occurrence);
        env.insert(binding.into(), Value::Term(Ty::Integer, b::integer_temporal_result(id as u64)));
        Ok(())
    }
}

pub(super) fn publish(
    choices: &[Occurrence], arrays: &BTreeSet<String>, names: &BTreeMap<String, String>,
) -> Result<(BTreeMap<u64, usize>, Vec<serde_json::Value>)> {
    let mut aliases = BTreeMap::new();
    let mut definitions = Vec::new();
    for (position, occurrence) in choices.iter().enumerate() {
        let source = names.get(&occurrence.members)
            .filter(|_| arrays.contains(&occurrence.members))
            .ok_or_else(|| fail("member choice source must be a published array parameter"))?;
        let empty = if let Some(integer) = occurrence.empty.as_lit().and_then(math::Lit::as_integer) {
            integer.to_string()
        } else if let Some(SymVarOrigin::Parameter { name, primitive_type }) = occurrence.empty.as_sym_var() {
            need(primitive_type == "integer", "member choice empty fallback must be a published exact integer")?;
            names.get(name).cloned().ok_or_else(|| fail("unbound published member choice fallback"))?
        } else {
            return Err(fail("member choice empty fallback must be a published integer or constant"));
        };
        aliases.insert(position as u64, position);
        definitions.push(json!({
            "id": format!("$t_{position}"), "op": "member_choice", "when": "true",
            "members": source, "empty": empty,
        }));
    }
    Ok((aliases, definitions))
}
