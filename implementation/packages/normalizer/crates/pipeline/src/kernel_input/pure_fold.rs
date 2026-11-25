//! Ordered, total exact-integer folds retained by the source evaluator.
use super::*;
use normalizer_ir::local::relation::{Op, Scalar};
use serde_json::json;

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Definition {
    source: String,
    initial: Expr,
    body: Expr,
}

fn local_body(expr: &Scalar, acc: &str, item: &str) -> Result<Expr> {
    Ok(match expr {
        Scalar::Path(parts) if parts.len() == 1 && parts[0] == acc => b::param("acc", "integer"),
        Scalar::Path(parts) if parts.len() == 1 && parts[0] == item => b::param("item", "integer"),
        Scalar::Literal(normalizer_ir::local::ScalarOperand::Integer(n)) => b::lit_integer(n.clone()),
        Scalar::Binary(op @ (Op::Add | Op::Sub | Op::Mul), left, right) => {
            let build = match op { Op::Add => b::add, Op::Sub => b::sub, _ => b::mul };
            build(local_body(left, acc, item)?, local_body(right, acc, item)?)
        }
        _ => return Err(fail("pure fold body requires closed exact-integer arithmetic")),
    })
}

impl Engine<'_> {
    pub(super) fn retain_pure_fold(
        &mut self, source: &str, acc: &str, item: &str,
        initial: Expr, body: &Scalar, guard: &Expr,
    ) -> Result<Value> {
        need(guard.as_bool() == Some(true) && acc != item,
            "pure fold requires unconditional evaluation over one ordered source array")?;
        let definition = Definition {
            source: source.into(),
            initial,
            body: math::normalize(local_body(body, acc, item)?),
        };
        let id = self.pure_folds.iter().position(|old| old == &definition).unwrap_or_else(|| {
            let id = self.pure_folds.len();
            self.pure_folds.push(definition);
            id
        }) as u64;
        Ok(Value::Term(Ty::Integer, b::integer_temporal_result(id)))
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum InitialKey {
    Integer(math::Integer),
    Parameter(String),
    AcquiredState(String),
    Prior(usize),
}

pub(super) fn publish(
    folds: &[Definition], expression: &Expr, arrays: &BTreeSet<String>,
    numbers: &BTreeSet<String>, names: &BTreeMap<String, String>,
    state_path: impl Fn(u64, &[String]) -> Option<String>,
) -> Result<(BTreeMap<u64, usize>, Vec<serde_json::Value>)> {
    let mut pending = BTreeSet::new();
    expression.walk(&mut |node| { if let Some(id) = integer_definition_id(node) { pending.insert(id); } });
    let mut todo: Vec<_> = pending.iter().copied().collect();
    while let Some(id) = todo.pop() {
        let def = folds.get(id as usize).ok_or_else(|| fail("undefined pure fold"))?;
        if let Some(dep) = integer_definition_id(&def.initial) {
            need(dep < id, "pure fold dependency must already be defined")?;
            if pending.insert(dep) { todo.push(dep); }
        }
    }
    let mut aliases = BTreeMap::new();
    let mut output = vec![];
    while !pending.is_empty() {
        let mut ready = vec![];
        for &id in &pending {
            let def = &folds[id as usize];
            need(arrays.contains(&def.source), "pure fold source requires a declared numeric input array")?;
            let initial = if let Some(dep) = integer_definition_id(&def.initial) {
                let Some(&slot) = aliases.get(&dep) else { continue; };
                InitialKey::Prior(slot)
            } else if let Some(n) = def.initial.as_lit().and_then(math::Lit::as_integer) {
                InitialKey::Integer(n)
            } else if let Some(SymVarOrigin::Parameter { name, primitive_type }) = def.initial.as_sym_var() {
                need(primitive_type == "integer" && numbers.contains(name), "pure fold initial parameter must be a declared number")?;
                InitialKey::Parameter(names.get(name).cloned().ok_or_else(|| fail("invalid fold input name"))?)
            } else if let Some(SymVarOrigin::External { binding_id, path, primitive_type }) = def.initial.as_sym_var() {
                need(primitive_type == "integer", "pure fold state initializer must be a declared number")?;
                InitialKey::AcquiredState(state_path(*binding_id, path)
                    .ok_or_else(|| fail("pure fold state initializer requires an acquired declared state field"))?)
            } else {
                return Err(fail("pure fold initial must be an integer, numeric parameter, acquired state field, or prior pure fold"));
            };
            let source = names.get(&def.source).cloned().ok_or_else(|| fail("invalid fold source name"))?;
            let body = math::canonical_order_key(&def.body, &math::DefaultSortContext);
            ready.push(((source, initial, body), id));
        }
        let (key, id) = ready.into_iter().min_by(|a,b| a.0.cmp(&b.0))
            .ok_or_else(|| fail("pure fold dependency cycle"))?;
        let slot = output.len();
        let initial = match &key.1 {
            InitialKey::Integer(n) => n.to_string(),
            InitialKey::Parameter(name) | InitialKey::AcquiredState(name) => name.clone(),
            InitialKey::Prior(slot) => format!("$t_{slot}"),
        };
        let mut ctx = crate::FormatCtx::for_business(BTreeMap::new(), BTreeMap::new());
        let body = crate::format_expr_with_ctx(&folds[id as usize].body, &mut ctx);
        output.push(json!({"id":format!("$t_{slot}"),"op":"fold","type":"number",
            "source":key.0,"initial":initial,"accumulator":"acc","element":"item","body":body}));
        aliases.insert(id, slot);
        pending.remove(&id);
    }
    Ok((aliases, output))
}
