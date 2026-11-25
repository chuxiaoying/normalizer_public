//! Independent source-owned choices. Unlike a pure fold, every occurrence has
//! an identity and stays in execution order even when candidate sets agree.
use super::*;
use normalizer_ir::local::MemberChoiceFallback;
use super::source_arrays::LocalIntArray;
use serde_json::json;

#[derive(Clone)]
enum Candidates {
    Input(String),
    Literal(Vec<i32>),
}

#[derive(Clone)]
pub(super) struct Occurrence {
    members: Candidates,
    empty: Expr,
    guard: Expr,
}

impl Engine<'_> {
    pub(super) fn choose_source_member(
        &mut self, binding: &str, members: &str, empty: &MemberChoiceFallback,
        method: &str, env: &mut Env, path: Expr,
    ) -> Result<()> {
        let empty = match empty {
            MemberChoiceFallback::Integer(value) => b::lit_integer(value.clone()),
            MemberChoiceFallback::Local(name) => {
                let value = env.get(name).ok_or_else(|| fail("unbound member choice empty fallback"))?;
                need(value.ty() == Ty::Integer, "member choice empty fallback must be an exact integer")?;
                value.scalar()?
            }
        };
        enum Choice {
            Fixed(Expr),
            Independent(Candidates),
        }
        let choice = match env.get(members) {
            Some(Value::LocalIntArray(LocalIntArray::Zeroed(0))) => Choice::Fixed(empty.clone()),
            Some(Value::LocalIntArray(LocalIntArray::Zeroed(_))) => Choice::Fixed(b::lit_int(0)),
            Some(Value::LocalIntArray(LocalIntArray::Initialized(values))) => match values.as_slice() {
                [] => Choice::Fixed(empty.clone()),
                [value, rest @ ..] if rest.iter().all(|other| other == value) =>
                    Choice::Fixed(b::lit_int(i64::from(*value))),
                _ => {
                    // The array has already been evaluated. For a choice with
                    // no order or distribution promise, only the support set
                    // of this literal is observable.
                    let mut support = values.clone();
                    support.sort_unstable();
                    support.dedup();
                    Choice::Independent(Candidates::Literal(support))
                }
            },
            Some(Value::Sequence {item: Ty::Integer, relation: RelationExpr::Base {name, ..}, ..}) => {
                need(self.methods[method].params().iter().any(|(ty, param)|
                    param == name && crate::normalized::numeric_array(ty)),
                    "member choice requires an acquired entry array")?;
                Choice::Independent(Candidates::Input(name.clone()))
            }
            _ => return Err(fail("member choice requires a direct integer source array")),
        };
        let members = match choice {
            Choice::Fixed(value) => {
                env.insert(binding.into(), Value::Term(Ty::Integer, value));
                return Ok(());
            }
            Choice::Independent(members) => members,
        };
        let occurrence = Occurrence {members, empty, guard: path};
        let id = self.member_choices.len();
        self.member_choices.push(occurrence);
        env.insert(binding.into(), Value::Term(Ty::Integer, b::integer_temporal_result(id as u64)));
        Ok(())
    }
}

pub(super) fn total_choices(choices: &[Occurrence]) -> BTreeSet<u64> {
    choices.iter().enumerate().filter_map(|(position, choice)|
        (choice.guard.as_bool() == Some(true)).then_some(position as u64)).collect()
}

/// Each reference must be reached only on paths on which its own occurrence exists.
pub(super) fn validate_return_choices(expression: &Expr, choices: &[Occurrence]) -> Result<()> {
    fn check(expression: &Expr, active: &Expr, choices: &[Occurrence]) -> Result<()> {
        if let Some(id) = integer_definition_id(expression) {
            let choice = choices.get(id as usize).ok_or_else(|| fail("unknown member choice reference"))?;
            return need(b::path_implies(active, &choice.guard), "member choice read outside its acquisition guard");
        }
        if let Some(branches) = expression.as_case_when() {
            let mut remaining = active.clone();
            for (test, value) in branches {
                check(test, &remaining, choices)?;
                check(value, &math::normalize(b::and(remaining.clone(), test.clone())), choices)?;
                remaining = math::normalize(b::and(remaining, b::not(test.clone())));
            }
        } else if let Some((op, left, right)) = expression.as_binop() {
            check(left, active, choices)?;
            let right_active = match op {
                math::BinOpKind::And => math::normalize(b::and(active.clone(), left.clone())),
                math::BinOpKind::Or => math::normalize(b::and(active.clone(), b::not(left.clone()))),
                _ => active.clone(),
            };
            check(right, &right_active, choices)?;
        } else if let Some((_, operand)) = expression.as_unary_op() {
            check(operand, active, choices)?;
        } else {
            let mut temporal = false;
            expression.walk(&mut |node| temporal |= integer_definition_id(node).is_some());
            need(!temporal, "member choice read under an uncertified operator")?;
        }
        Ok(())
    }
    check(expression, &b::lit_bool(true), choices)
}

pub(super) fn publish(
    choices: &[Occurrence], arrays: &BTreeSet<String>, names: &BTreeMap<String, String>,
    state_path: impl Fn(u64, &[String]) -> Option<String>,
) -> Result<(BTreeMap<u64, usize>, Vec<serde_json::Value>)> {
    let mut aliases = BTreeMap::new();
    let mut definitions = Vec::new();
    for (position, occurrence) in choices.iter().enumerate() {
        let source = match &occurrence.members {
            Candidates::Input(name) => names.get(name).filter(|_| arrays.contains(name)).cloned()
                .ok_or_else(|| fail("member choice source must be a published array parameter"))?,
            Candidates::Literal(values) => format!("[{}]", values.iter().map(i32::to_string).collect::<Vec<_>>().join(", ")),
        };
        let empty = if let Some(integer) = occurrence.empty.as_lit().and_then(math::Lit::as_integer) {
            integer.to_string()
        } else if let Some(SymVarOrigin::Parameter { name, primitive_type }) = occurrence.empty.as_sym_var() {
            need(primitive_type == "integer", "member choice empty fallback must be a published exact integer")?;
            names.get(name).cloned().ok_or_else(|| fail("unbound published member choice fallback"))?
        } else if let Some(SymVarOrigin::External { binding_id, path, primitive_type }) = occurrence.empty.as_sym_var() {
            need(primitive_type == "integer", "member choice state fallback must be a declared integer")?;
            state_path(*binding_id, path)
                .ok_or_else(|| fail("member choice fallback requires an acquired declared state field"))?
        } else {
            return Err(fail("member choice fallback must be a constant, acquired integer parameter or state field"));
        };
        let mut entry_guard = true;
        occurrence.guard.walk(&mut |node| {
            entry_guard &= node.as_lit().is_some() || node.as_binop().is_some() || node.as_unary_op().is_some()
                || matches!(node.as_sym_var(), Some(SymVarOrigin::Parameter { name, primitive_type })
                    if matches!(primitive_type.as_str(), "integer" | "boolean") && names.contains_key(name));
        });
        need(entry_guard, "member choice guard requires acquired entry scalars")?;
        let mut ctx = crate::FormatCtx::for_business(names.clone(), BTreeMap::new());
        let guard = super::business_event::condition_text(&occurrence.guard, &mut ctx);
        aliases.insert(position as u64, position);
        definitions.push(json!({
            "id": format!("$t_{position}"), "op": "member_choice", "when": guard,
            "members": source, "empty": empty,
        }));
    }
    Ok((aliases, definitions))
}
