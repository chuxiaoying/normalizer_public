//! Shape-preserving, nonescaping tuple construction and scoped projection.
use super::*;

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum WirePlan {
    Literal { items: Vec<Slot> },
    Branch { cond: Box<Node>, yes: Box<WirePlan>, no: Box<WirePlan> },
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Slot {
    Undefined,
    Text { value: String },
    Record { fields: BTreeMap<String, ScalarField> },
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(untagged)]
enum ScalarField {
    Flag(bool),
    Text(String),
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Binding {
    IsUndefined { name: String, index: usize },
    SlotTruth { name: String, index: usize },
    OptionalFlag { name: String, index: usize, field: String },
    RestLength { name: String, from: usize },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireObservation {
    tuple: WirePlan,
    observations: Vec<Binding>,
    body: Box<Node>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(try_from = "WireObservation")]
pub(super) struct Observation {
    pub(super) tuple: Plan,
    pub(super) body: Box<Node>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Plan {
    Literal(Vec<(String, Ty, Expr)>),
    Branch { cond: Box<Node>, yes: Box<Plan>, no: Box<Plan> },
}

fn admit_plan(plan: WirePlan, observations: &[Binding]) -> Result<Plan> {
    Ok(match plan {
        WirePlan::Branch { cond, yes, no } => {
            if !super::tasks::expression_is_closed(&cond) {
                return Err("tuple branch requires a pure condition".into());
            }
            Plan::Branch {
                cond,
                yes: Box::new(admit_plan(*yes, observations)?),
                no: Box::new(admit_plan(*no, observations)?),
            }
        }
        WirePlan::Literal { items } => {
            let fields = observations.iter().map(|binding| {
                let field = match binding {
                    Binding::IsUndefined { name, index } => {
                        let flag = matches!(items.get(*index), None | Some(Slot::Undefined));
                        (name, Ty::Bool, b::lit_bool(flag))
                    }
                    Binding::SlotTruth { name, index } => {
                        let flag = match items.get(*index) {
                            None | Some(Slot::Undefined) => false,
                            Some(Slot::Text { value }) => !value.is_empty(),
                            Some(Slot::Record { .. }) => true,
                        };
                        (name, Ty::Bool, b::lit_bool(flag))
                    }
                    Binding::OptionalFlag { name, index, field } => {
                        let flag = match items.get(*index) {
                            None | Some(Slot::Undefined) => false,
                            Some(Slot::Record { fields }) => match fields.get(field) {
                                None => false,
                                Some(ScalarField::Flag(value)) => *value,
                                Some(ScalarField::Text(_)) => return Err("optional tuple property is not Boolean".into()),
                            },
                            _ => return Err("optional tuple receiver is not a record".to_owned()),
                        };
                        (name, Ty::Bool, b::lit_bool(flag))
                    }
                    Binding::RestLength { name, from } => {
                        let count = i64::try_from(items.get(*from..).unwrap_or_default().len())
                            .map_err(|_| "tuple length range".to_owned())?;
                        (name, Ty::Integer, b::lit_int(count))
                    }
                };
                Ok((field.0.clone(), field.1, field.2))
            }).collect::<Result<_>>()?;
            Plan::Literal(fields)
        }
    })
}

impl TryFrom<WireObservation> for Observation {
    type Error = String;

    fn try_from(raw: WireObservation) -> Result<Self> {
        if !super::tasks::expression_is_closed(&raw.body) {
            return Err("tuple observation requires a pure scalar body".into());
        }
        Ok(Self { tuple: admit_plan(raw.tuple, &raw.observations)?, body: raw.body })
    }
}

impl Engine<'_> {
    pub(super) fn observe_tuple(
        &mut self, plan: &Plan, body: &Node, env: &Env, path: Expr,
    ) -> Result<Value> {
        self.budget()?;
        match plan {
            Plan::Branch { cond, yes, no } => {
                let value = self.node(cond, &mut env.clone(), path.clone())?;
                need(value.ty() == Ty::Bool, "tuple branch requires Boolean")?;
                let condition = self.condition_under_path(value.scalar()?, &path);
                if let Some(taken) = condition.as_bool() {
                    return self.observe_tuple(if taken { yes } else { no }, body, env, path);
                }
                let a = self.observe_tuple(yes, body, env, b::and(path.clone(), condition.clone()))?;
                let z = self.observe_tuple(no, body, env, b::and(path, b::not(condition.clone())))?;
                self.choose(condition, a, z)
            }
            Plan::Literal(fields) => {
                let mut scope = env.clone();
                for (name, ty, value) in fields {
                    Self::bind_local(&mut scope, name, Value::Term(ty.clone(), value.clone()))?;
                }
                self.node(body, &mut scope, path)
            }
        }
    }
}
