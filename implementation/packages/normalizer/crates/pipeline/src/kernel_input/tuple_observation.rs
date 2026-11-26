//! Shape-preserving, nonescaping tuple construction and scoped projection.
use super::*;

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Plan {
    Literal { items: Vec<Slot> },
    Branch { cond: Box<Node>, yes: Box<Plan>, no: Box<Plan> },
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Slot {
    Undefined,
    Text { value: String },
    Record { fields: BTreeMap<String, bool> },
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Binding {
    IsUndefined { name: String, index: usize },
    OptionalFlag { name: String, index: usize, field: String },
    RestLength { name: String, from: usize },
}

impl Engine<'_> {
    pub(super) fn observe_tuple(
        &mut self, plan: &Plan, observations: &[Binding], body: &Node, env: &Env, path: Expr,
    ) -> Result<Value> {
        self.budget()?;
        match plan {
            Plan::Branch { cond, yes, no } => {
                need(super::tasks::expression_is_closed(cond), "tuple branch requires a pure condition")?;
                let value = self.node(cond, &mut env.clone(), path.clone())?;
                need(value.ty() == Ty::Bool, "tuple branch requires Boolean")?;
                let condition = self.condition_under_path(value.scalar()?, &path);
                let a = self.observe_tuple(yes, observations, body, env, b::and(path.clone(), condition.clone()))?;
                let z = self.observe_tuple(no, observations, body, env, b::and(path, b::not(condition.clone())))?;
                self.choose(condition, a, z)
            }
            Plan::Literal { items } => {
                let mut scope = env.clone();
                for binding in observations {
                    let (name, value) = match binding {
                        Binding::IsUndefined { name, index } => {
                            let flag = matches!(items.get(*index), None | Some(Slot::Undefined));
                            (name, Value::Term(Ty::Bool, b::lit_bool(flag)))
                        }
                        Binding::OptionalFlag { name, index, field } => {
                            let flag = match items.get(*index) {
                                None | Some(Slot::Undefined) => false,
                                Some(Slot::Record { fields }) => fields.get(field).copied().unwrap_or(false),
                                _ => return Err(fail("optional tuple receiver is not a record")),
                            };
                            (name, Value::Term(Ty::Bool, b::lit_bool(flag)))
                        }
                        Binding::RestLength { name, from } => {
                            let tail = items.get(*from..).unwrap_or_default();
                            let count = i64::try_from(tail.len()).map_err(|_| fail("tuple length range"))?;
                            (name, Value::Term(Ty::Integer, b::lit_int(count)))
                        }
                    };
                    Self::bind_local(&mut scope, name, value)?;
                }
                self.node(body, &mut scope, path)
            }
        }
    }
}
