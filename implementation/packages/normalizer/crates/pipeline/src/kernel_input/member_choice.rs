//! Independent source-owned choices. Unlike a pure fold, every occurrence has
//! an identity and stays in execution order even when candidate sets agree.
use super::*;
use normalizer_ir::local::MemberChoiceFallback;
use super::source_arrays::LocalIntArray;

#[derive(Clone, PartialEq, Eq)]
pub(super) enum Candidates {
    Input(String),
    Literal(Vec<i32>),
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Occurrence {
    pub(super) members: Candidates,
    pub(super) empty: Expr,
    pub(super) guard: Expr,
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
        let id = self.temporal_definitions.len();
        self.temporal_definitions.push(super::temporal_graph::Definition::Choice(occurrence));
        env.insert(binding.into(), Value::Term(Ty::Integer, b::integer_temporal_result(id as u64)));
        Ok(())
    }
}
