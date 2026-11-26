//! Bind Java external methods to the same typed completion and ordered event
//! sequence used by the other source front ends.
use super::*;
use crate::normalized::source_ports::SourcePort;

fn domain(ty: &JavaType, program: &JavaProgram) -> Result<Ty> {
    match ty {
        JavaType::Void => Ok(Ty::Unit),
        t if crate::normalized::integral(t) => Ok(Ty::Integer),
        JavaType::Primitive(n) if n == "boolean" => Ok(Ty::Bool),
        JavaType::Class(n) if matches!(n.as_str(), "String" | "java.lang.String") => Ok(Ty::Text),
        JavaType::Class(name) => {
            let class = program
                .find_class(name)
                .ok_or_else(|| fail("unknown external result domain"))?;
            need(
                !class.is_interface() && !class.fields().is_empty() && class.methods().is_empty(),
                "port result requires a declared data record",
            )?;
            let fields = class
                .fields()
                .iter()
                .map(|field| {
                    let t = field.java_type();
                    need(
                        crate::normalized::integral(t)
                            || matches!(t,JavaType::Primitive(n) if n=="boolean")
                            || matches!(t,JavaType::Class(n) if n=="String"||n=="java.lang.String"),
                        "port record field requires a supported scalar",
                    )?;
                    Ok((field.name().to_owned(), domain(t, program)?))
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
                .map(|(ty, name)| {
                    let ty = domain(ty, program)?;
                    need(
                        matches!(ty, Ty::Integer | Ty::Bool | Ty::Text),
                        "source port arguments must be immutable scalars",
                    )?;
                    Ok(Parameter {
                        name: name.clone(),
                        typ: ty.describe(),
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
                domain(&p.returns, program)?.describe(),
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
        args: Vec<Value>,
        path: Expr,
    ) -> Result<Value> {
        let contract = &self.checked_ports[id];
        need(
            args.len() == contract.parameters.len(),
            "source port argument count mismatch",
        )?;
        let mut values = vec![];
        for (arg, expected) in args.iter().zip(&contract.parameters) {
            need(
                arg.ty() == *expected,
                "source port argument type mismatch",
            )?;
            values.push(arg.scalar()?);
        }
        let guard = self.normal_path(path);
        let bind = self.fresh_binding();
        let spec = &self.checked_ports[id].completion;
        let receipt = realtime_state::EffectReceipt::tagged(bind);
        let response = receipt.response().clone();
        self.realtime_effects
            .record(guard.clone(), id, id, values, receipt);
        for (class, failed, payload) in spec.throw_variants(&response) {
            let condition = math::normalize(b::and(guard.clone(), failed.clone()));

            self.realtime_throws.push(condition.clone());
            self.realtime_completion = self
                .realtime_completion
                .replace_continue_with_throw(condition, &class, payload);
        }
        // A non-throwing integer port exposes the acquired numeric value of
        // this receipt, not the completion control token. The identity and
        // guarded occurrence in S stay exactly the same.
        let value = if spec.returns == Ty::Integer && !spec.throwing() {
            b::sym_e(bind, "integer")
        } else {
            spec.normal_value(&response)
        };
        Ok(Value::Term(spec.returns.clone(), value))
    }
}
