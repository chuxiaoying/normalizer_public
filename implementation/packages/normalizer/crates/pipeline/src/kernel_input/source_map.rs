//! A local, nonescaping text map. Presence and nullable payload are distinct.
use super::*;
use normalizer_ir::local::MapText;

#[derive(Clone, PartialEq)]
struct Entry {
    present: Expr,
    payload: Expr,
}

#[derive(Clone, Default, PartialEq)]
pub(super) struct LocalTextMap(BTreeMap<String, Entry>);

pub(super) fn declared_map(declaration: &JavaType) -> bool {
    matches!(declaration, JavaType::Generic(name, arguments)
        if name == "java.util.HashMap" && arguments.len() == 2
            && arguments.iter().all(declared_text))
}

pub(super) fn declared_text(declaration: &JavaType) -> bool {
    matches!(declaration, JavaType::Class(name) if matches!(name.as_str(), "String" | "java.lang.String"))
}

pub(super) fn text_operand(operand: &MapText, env: &Env) -> Result<Expr> {
    match operand {
        MapText::Null => Ok(b::lit_null()),
        MapText::Literal(text) => Ok(b::lit_str(text)),
        MapText::Local(name) => {
            let Some(Value::Term(Ty::Text, value)) = env.get(name) else {
                unreachable!();
            };
            Ok(value.clone())
        }
    }
}

impl LocalTextMap {
    pub(super) fn put(&mut self, key: String, value: Expr) {
        self.0.insert(key, Entry {present: b::lit_bool(true), payload: value});
    }

    pub(super) fn get_or(&self, key: &str, fallback: Expr) -> Expr {
        let Some(entry) = self.0.get(key) else { return fallback; };
        branch(entry.present.clone(), entry.payload.clone(), fallback)
    }

    pub(super) fn join(self, condition: Expr, mut other: Self) -> Self {
        let mut joined = BTreeMap::new();
        for (key, yes) in self.0 {
            let entry = if let Some(no) = other.0.remove(&key) {
                Entry {
                    present: branch(condition.clone(), yes.present, no.present),
                    payload: branch(condition.clone(), yes.payload, no.payload),
                }
            } else {
                Entry {
                    present: math::normalize(b::and(condition.clone(), yes.present)),
                    payload: yes.payload,
                }
            };
            joined.insert(key, entry);
        }
        let inverse = b::not(condition);
        joined.extend(other.0.into_iter().map(|(key, no)| (key, Entry {
            present: math::normalize(b::and(inverse.clone(), no.present)),
            payload: no.payload,
        })));
        Self(joined)
    }
}

fn branch(guard: Expr, yes: Expr, no: Expr) -> Expr {
    math::normalize(b::case_when(vec![(guard, yes)], no))
}
