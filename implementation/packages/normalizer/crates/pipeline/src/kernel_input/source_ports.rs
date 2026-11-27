//! Bind Java external methods to the same typed completion and ordered event
//! sequence used by the other source front ends.
use super::*;
use crate::normalized::source_ports::SourcePort;

fn scalar_domain(ty: &JavaType) -> Option<Ty> {
    match ty {
        t if crate::normalized::integral(t) => Some(Ty::Integer),
        JavaType::Primitive(n) if n == "boolean" => Some(Ty::Bool),
        JavaType::Class(n) if matches!(n.as_str(), "String" | "java.lang.String") => Some(Ty::Text),
        _ => None,
    }
}

fn domain(ty: &JavaType, program: &JavaProgram) -> Result<Ty> {
    if let Some(ty) = scalar_domain(ty) {
        return Ok(ty);
    }
    match ty {
        JavaType::Void => Ok(Ty::Unit),
        JavaType::Class(name) => {
            let class = program
                .find_class(name)
                .ok_or_else(|| fail("unknown external result domain"))?;
            need(
                !class.is_interface() && class.fields().iter().any(|field| !field.is_static())
                    && class.methods().iter().all(|method| crate::normalized::plain_source_data_method(class, method)),
                "port data needs source-declared fields and closed accessors",
            )?;
            let fields = class
                .fields()
                .iter()
                .filter(|field| !field.is_static())
                .map(|field| {
                    let ty = scalar_domain(field.java_type())
                        .ok_or_else(|| fail("port record field requires a supported scalar"))?;
                    Ok((field.name().to_owned(), ty))
                })
                .collect::<Result<_>>()?;
            Ok(Ty::Record(fields))
        }
        _ => Err(fail("unsupported source port type")),
    }
}

pub(super) fn contracts(
    program: &JavaProgram,
    ports: &BTreeMap<String, SourcePort>,
) -> Result<BTreeMap<String, actions::Port>> {
    ports
        .iter()
        .map(|(id, p)| {
            let params = p
                .params
                .iter()
                .map(|(source_type, name)| {
                    let ty = domain(source_type, program)?;
                    let immutable_value = matches!(source_type, JavaType::Class(name)
                        if program.find_class(name).is_some_and(|class| class.is_record()));
                    need(
                        matches!(ty, Ty::Integer | Ty::Bool | Ty::Text)
                            || matches!(ty, Ty::Record(_)) && immutable_value,
                        "source port arguments must be immutable scalars or immutable records; a mutable object does not declare when its request fields are read",
                    )?;
                    Ok(Parameter {
                        name: name.clone(),
                        typ: ty,
                    })
                })
                .collect::<Result<_>>()?;
            let origin = Origin {
                path: p.source_path.clone(),
                excerpt: p.endpoint.rsplit('.').next().unwrap().into(),
            };
            let port = actions::Port::from_source(
                origin,
                p.endpoint.clone(),
                params,
                domain(&p.returns, program)?,
                p.throws.clone(),
            );
            Ok((id.clone(), port))
        })
        .collect()
}

impl Engine<'_> {
    pub(super) fn source_port_call(
        &mut self,
        id: &str,
        rooted_identity: Option<String>,
        args: Vec<Value>,
        path: Expr,
    ) -> Result<Value> {
        let guard = self.normal_path(path);
        let contract = &self.checked_ports[id];
        if guard.as_bool() == Some(false) {
            // No call occurs, so this carrier cannot denote a receipt.
            return Ok(Value::Term(contract.completion.returns.clone(), b::lit_null()));
        }
        need(args.len() == contract.parameters.len(), "source port argument count mismatch")?;
        let parameters = contract.parameters.clone();
        let declarations: Vec<_> = self.project.source_ports[id].params.iter().map(|(ty, _)| ty.clone()).collect();
        let mut values = Vec::with_capacity(args.len());
        for ((arg, expected), declared) in args.iter().zip(&parameters).zip(&declarations) {
            if let Ty::Record(fields) = expected {
                if arg.ty() == Ty::Ref {
                    values.push(self.freeze_source_port_record(arg.scalar()?, declared, fields, &guard)?);
                    continue;
                }
            }
            need(arg.ty() == *expected, "source port argument type mismatch")?;
            values.push(arg.scalar()?);
        }
        let bind = self.fresh_binding();
        let spec = &self.checked_ports[id].completion;
        let receipt = realtime_state::EffectReceipt::tagged(bind);
        let response = receipt.response().clone();
        self.realtime_effects
            .record_source_port(guard.clone(), id, values, receipt, spec.clone(), rooted_identity);
        if spec.throwing() {
            // These failures are disjoint tags of this one receipt. Install
            // the partition together: list order is not exception priority.
            let mut failure = b::lit_bool(false);
            let mut alternatives = Vec::new();
            for (class, failed, payload) in spec.throw_variants(&response) {
                let condition = math::normalize(b::and(guard.clone(), failed));
                failure = b::or(failure, condition.clone());
                self.realtime_throws.push(condition.clone());
                alternatives.push(realtime_state::CompletionAlternative {
                    guard: condition,
                    outcome: realtime_state::CompletionOutcome::Throw { class, payload },
                });
            }
            alternatives.push(realtime_state::CompletionAlternative {
                guard: math::normalize(b::not(failure)),
                outcome: realtime_state::CompletionOutcome::Continue,
            });
            self.realtime_completion = self.realtime_completion.replace_continue(
                realtime_state::Completion::from_alternatives(alternatives),
            );
        }
        // A non-throwing integer port exposes the acquired numeric value of
        // this receipt, not the completion control token. The identity and
        // guarded occurrence in S stay exactly the same.
        let value = if spec.returns == Ty::Integer && !spec.throwing() {
            b::sym_e(bind, ScalarSymbolType::Integer)
        } else {
            spec.normal_value(&response)
        };
        Ok(Value::Term(spec.returns.clone(), value))
    }
}
