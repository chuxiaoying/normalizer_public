//! Independent source-owned choices. Every occurrence has
//! an identity and stays in execution order even when candidate sets agree.
use super::*;
use normalizer_ir::local::MemberChoiceFallback;

#[derive(Clone, PartialEq, Eq)]
pub(super) enum Candidates {
    Input(String),
    Literal(Vec<math::Integer>),
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Occurrence {
    pub(super) members: Candidates,
    pub(super) empty: Fallback,
    pub(super) guard: Expr,
}

#[derive(Clone, PartialEq, Eq)]
pub(super) enum Fallback {
    Integer(Expr),
    RecordParameter { owner: String, name: String },
}

impl Engine<'_> {
    pub(super) fn choose_source_member(
        &mut self,
        binding: &str,
        members: &str,
        empty: &MemberChoiceFallback,
        declared: &JavaType,
        method: &str,
        env: &mut Env,
        path: Expr,
    ) -> Result<()> {
        if let Some(owner) = self.project.source_owner(declared) {
            return self.choose_record_member(binding, members, empty, owner, method, env, path);
        }
        let mut empty = match empty {
            MemberChoiceFallback::Integer(value) => b::lit_integer(value.clone()),
            MemberChoiceFallback::Local(name) => {
                let value = env
                    .get(name)
                    .ok_or_else(|| fail("unbound member choice empty fallback"))?;
                need(
                    value.ty() == Ty::Integer,
                    "member choice empty fallback must be an exact integer",
                )?;
                value.scalar()?
            }
        };
        // MA-02: on an acquired equality point the exact constant is the
        // smaller representative of this integer fallback, not a new read.
        let mut point = None;
        path.walk(&mut |predicate| {
            let Some((BinOpKind::Eq, left, right)) = predicate.as_binop() else {
                return;
            };
            let literal = if left == &empty {
                right
            } else if right == &empty {
                left
            } else {
                return;
            };
            if literal.as_lit().and_then(math::Lit::as_integer).is_some()
                && b::path_implies(&path, predicate)
            {
                point = Some(literal.clone());
            }
        });
        if let Some(point) = point {
            empty = point;
        }
        enum Choice {
            Fixed(Expr),
            Independent(Candidates),
        }
        let choice = match env.get(members) {
            Some(Value::LocalIntArray(array)) => {
                // A choice uses the acquired contents at this instruction,
                // not the values that happened to initialize its aliases.
                let snapshot = self.snapshot_local_choice(array, &path)?;
                match snapshot.as_slice() {
                    [] => Choice::Fixed(empty.clone()),
                    [value, rest @ ..] if rest.iter().all(|other| other == value) => {
                        Choice::Fixed(value.clone())
                    }
                    _ => {
                        // No order or distribution is observed in this
                        // contract. Deduplicate the actual constant support.
                        let mut support = snapshot.iter().map(|value| {
                            value.as_lit().and_then(math::Lit::as_integer)
                                .ok_or_else(|| fail("independent local member choice requires a certified constant support set"))
                        }).collect::<Result<Vec<_>>>()?;
                        support.sort_unstable();
                        support.dedup();
                        Choice::Independent(Candidates::Literal(support))
                    }
                }
            }
            Some(Value::Sequence {
                item: Ty::Integer,
                name,
                null,
                ..
            }) => {
                need(
                    b::path_implies(&path, &b::not(null.clone())),
                    "member choice needs an acquired non-null source array",
                )?;
                need(
                    self.methods[method]
                        .syntax
                        .params()
                        .iter()
                        .any(|(ty, param)| param == name && crate::normalized::numeric_array(ty)),
                    "member choice requires an acquired entry array",
                )?;
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
        let occurrence = Occurrence {
            members,
            empty: Fallback::Integer(empty),
            guard: path,
        };
        let id = self.temporal_definitions.len();
        self.temporal_definitions
            .push(super::temporal_graph::Definition::Choice(occurrence));
        env.insert(
            binding.into(),
            Value::Term(Ty::Integer, b::integer_temporal_result(id as u64)),
        );
        Ok(())
    }

    fn choose_record_member(
        &mut self,
        binding: &str,
        members: &str,
        empty: &MemberChoiceFallback,
        owner: &str,
        method: &str,
        env: &mut Env,
        path: Expr,
    ) -> Result<()> {
        let Some(Value::Sequence {
            item: Ty::Ref,
            name,
            ..
        }) = env.get(members)
        else {
            return Err(fail(
                "record choice requires a declared record array parameter",
            ));
        };
        need(self.methods[method].syntax.params().iter().any(|(ty, param)|
            param == name && matches!(ty, JavaType::Array(item) if self.project.source_owner(item) == Some(owner))),
            "record choice requires an array of its declared record type")?;
        let MemberChoiceFallback::Local(fallback) = empty else {
            return Err(fail(
                "record choice needs a same-type record parameter fallback",
            ));
        };
        let fallback = env
            .get(fallback)
            .ok_or_else(|| fail("unbound record choice fallback"))?
            .scalar()?;
        let (fallback_owner, fallback_name) = self
            .source_objects
            .borrowed_parameter(&fallback)
            .ok_or_else(|| fail("record choice needs a same-type record parameter fallback"))?;
        need(
            fallback_owner == owner
                && self.methods[method]
                    .syntax
                    .params()
                    .iter()
                    .any(|(ty, name)| {
                        name == fallback_name && self.project.source_owner(ty) == Some(owner)
                    })
                && self.methods[method]
                    .syntax
                    .param_annotations(fallback_name)
                    .is_empty(),
            "record choice needs a same-type nonnullable record parameter fallback",
        )?;
        let class = self
            .program
            .find_class(owner)
            .ok_or_else(|| fail("missing record choice layout"))?;
        let fields = class
            .fields()
            .iter()
            .map(|field| {
                let ty = self.project.parameter_type(field.java_type())?;
                need(
                    matches!(ty, Ty::Integer | Ty::Bool | Ty::Text),
                    "record choice supports only declared immutable scalar fields",
                )?;
                Ok((field.name().to_owned(), ty))
            })
            .collect::<Result<BTreeMap<_, _>>>()?;
        let occurrence = Occurrence {
            members: Candidates::Input(name.clone()),
            empty: Fallback::RecordParameter {
                owner: owner.into(),
                name: fallback_name.into(),
            },
            guard: path,
        };
        let id = self.temporal_definitions.len();
        self.temporal_definitions
            .push(super::temporal_graph::Definition::Choice(occurrence));
        env.insert(
            binding.into(),
            Value::Term(Ty::Record(fields), b::record_temporal_result(id as u64)),
        );
        Ok(())
    }
}
