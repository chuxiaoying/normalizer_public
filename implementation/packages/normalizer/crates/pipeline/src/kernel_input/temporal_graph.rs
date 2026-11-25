//! One identity and publication graph for deterministic folds and independent choices.
use super::*;
use super::member_choice::{Candidates, Occurrence};
use serde_json::{json, Value as Json};

#[derive(PartialEq, Eq)]
pub(super) enum Definition {
    Fold(super::pure_fold::Definition),
    Choice(Occurrence),
}

impl Definition {
    fn operand(&self) -> &Expr {
        match self {
            Self::Fold(node) => &node.initial,
            Self::Choice(node) => &node.empty,
        }
    }

    fn is_total(&self) -> bool {
        match self {
            Self::Fold(_) => true,
            Self::Choice(node) => node.guard.as_bool() == Some(true),
        }
    }
}

#[derive(Default)]
pub(super) struct Publication {
    pub(super) aliases: BTreeMap<u64, usize>,
    pub(super) total: BTreeSet<u64>,
    pub(super) rows: Vec<Json>,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Operand {
    Integer(math::Integer),
    Parameter(String),
    Acquired(String),
    Prior(usize),
}

impl Operand {
    fn render(&self) -> String {
        match self {
            Self::Integer(n) => n.to_string(),
            Self::Parameter(name) | Self::Acquired(name) => name.clone(),
            Self::Prior(slot) => format!("$t_{slot}"),
        }
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum Ready {
    Fold(String, Operand, String),
    Choice(u64),
}

pub(super) fn publish(
    nodes: &[Definition], expression: &Expr, arrays: &BTreeSet<String>,
    numbers: &BTreeSet<String>, names: &BTreeMap<String, String>,
    acquired_path: impl Fn(u64, &[String]) -> Option<String>,
) -> Result<Publication> {
    let mut pending: BTreeSet<_> = nodes.iter().enumerate().filter_map(|(id, node)|
        matches!(node, Definition::Choice(_)).then_some(id as u64)).collect();
    expression.walk(&mut |node| { pending.extend(integer_definition_id(node)); });
    let mut inspect: Vec<_> = pending.iter().copied().collect();
    while let Some(id) = inspect.pop() {
        let node = nodes.get(id as usize).ok_or_else(|| fail("undefined temporal definition"))?;
        if let Some(dep) = integer_definition_id(node.operand()) {
            need(dep < id, "temporal dependency must already be defined")?;
            if pending.insert(dep) { inspect.push(dep); }
        }
    }
    let mut published = Publication::default();
    let array_name = |name: &str| names.get(name).filter(|_| arrays.contains(name))
        .ok_or_else(|| fail("temporal source requires a published array parameter"));
    while !pending.is_empty() {
        let mut ready = Vec::new();
        let mut first_choice = true;
        for &id in &pending {
            let node = &nodes[id as usize];
            if matches!(node, Definition::Choice(_)) && !std::mem::take(&mut first_choice) { continue; }
            let operand = node.operand();
            let value = if let Some(dep) = integer_definition_id(operand) {
                let Some(&slot) = published.aliases.get(&dep) else { continue; };
                if let Definition::Choice(prior) = &nodes[dep as usize] {
                    let available = match node {
                        Definition::Choice(choice) => b::path_implies(&choice.guard, &prior.guard),
                        Definition::Fold(_) => prior.guard.as_bool() == Some(true),
                    };
                    need(available, "temporal operand is not acquired on this definition's path")?;
                }
                Operand::Prior(slot)
            } else if let Some(n) = operand.as_lit().and_then(math::Lit::as_integer) {
                Operand::Integer(n)
            } else if let Some(SymVarOrigin::Parameter {name, primitive_type}) = operand.as_sym_var() {
                need(primitive_type == "integer" && numbers.contains(name),
                    "temporal parameter must be a declared number")?;
                Operand::Parameter(names.get(name).cloned().ok_or_else(|| fail("invalid temporal input name"))?)
            } else if let Some(SymVarOrigin::External {binding_id, path, primitive_type}) = operand.as_sym_var() {
                need(primitive_type == "integer", "temporal acquired value must be a declared number")?;
                Operand::Acquired(acquired_path(*binding_id, path)
                    .ok_or_else(|| fail("temporal input requires an acquired declared integer"))?)
            } else {
                return Err(fail(match node {
                    Definition::Fold(_) => "pure fold initial must be an integer, numeric parameter, acquired state field, or prior pure fold",
                    Definition::Choice(_) => "member choice fallback must be a constant or an acquired integer",
                }));
            };
            let key = match node {
                Definition::Fold(fold) => Ready::Fold(array_name(&fold.source)?.clone(), value.clone(),
                    math::canonical_order_key(&fold.body, &math::DefaultSortContext)),
                Definition::Choice(_) => Ready::Choice(id),
            };
            ready.push((key, id, value));
        }
        let (_, id, value) = ready.into_iter().min_by(|a,b| a.0.cmp(&b.0))
            .ok_or_else(|| fail("temporal definition dependency cycle"))?;
        let slot = published.rows.len();
        let label = format!("$t_{slot}");
        let row = match &nodes[id as usize] {
            Definition::Fold(fold) => {
                let mut ctx = crate::FormatCtx::for_business(BTreeMap::new(), BTreeMap::new());
                let body = crate::format_expr_with_ctx(&fold.body, &mut ctx);
                json!({"id":label,"op":"fold","type":"number","source":array_name(&fold.source)?,
                    "initial":value.render(),"accumulator":"acc","element":"item","body":body})
            }
            Definition::Choice(choice) => {
                let mut entry_guard = true;
                choice.guard.walk(&mut |node| {
                    entry_guard &= node.as_lit().is_some() || node.as_binop().is_some() || node.as_unary_op().is_some()
                        || matches!(node.as_sym_var(), Some(SymVarOrigin::Parameter { name, primitive_type })
                            if matches!(primitive_type.as_str(), "integer" | "boolean") && names.contains_key(name));
                });
                need(entry_guard, "member choice guard requires acquired entry scalars")?;
                let members = match &choice.members {
                    Candidates::Input(name) => array_name(name)?.clone(),
                    Candidates::Literal(values) => format!("[{}]", values.iter().map(i32::to_string).collect::<Vec<_>>().join(", ")),
                };
                let mut ctx = crate::FormatCtx::for_business(names.clone(), BTreeMap::new());
                let guard = super::business_event::condition_text(&choice.guard, &mut ctx);
                json!({"id":label,"op":"member_choice","when":guard,"members":members,"empty":value.render()})
            }
        };
        published.aliases.insert(id, slot);
        if nodes[id as usize].is_total() { published.total.insert(id); }
        published.rows.push(row);
        pending.remove(&id);
    }
    Ok(published)
}

/// Prove every partial reference is reached only after its occurrence.
pub(super) fn validate_reads(expression: &Expr, nodes: &[Definition]) -> Result<()> {
    fn check(expression: &Expr, active: &Expr, nodes: &[Definition]) -> Result<()> {
        if let Some(id) = integer_definition_id(expression) {
            let def = nodes.get(id as usize).ok_or_else(|| fail("unknown temporal reference"))?;
            return match def {
                Definition::Fold(_) => Ok(()),
                Definition::Choice(choice) => need(b::path_implies(active, &choice.guard),
                    "member choice read outside its acquisition guard"),
            };
        }
        if let Some(branches) = expression.as_case_when() {
            let mut remaining = active.clone();
            for (test, value) in branches {
                check(test, &remaining, nodes)?;
                check(value, &math::normalize(b::and(remaining.clone(), test.clone())), nodes)?;
                remaining = math::normalize(b::and(remaining, b::not(test.clone())));
            }
        } else if let Some((op, left, right)) = expression.as_binop() {
            check(left, active, nodes)?;
            let right_active = match op {
                math::BinOpKind::And => math::normalize(b::and(active.clone(), left.clone())),
                math::BinOpKind::Or => math::normalize(b::and(active.clone(), b::not(left.clone()))),
                _ => active.clone(),
            };
            check(right, &right_active, nodes)?;
        } else if let Some((_, operand)) = expression.as_unary_op() {
            check(operand, active, nodes)?;
        } else {
            let mut partial = false;
            expression.walk(&mut |node| {
                if let Some(id) = integer_definition_id(node) {
                    partial |= nodes.get(id as usize).is_none_or(|node| !node.is_total());
                }
            });
            need(!partial, "member choice read under an uncertified operator")?;
        }
        Ok(())
    }
    check(expression, &b::lit_bool(true), nodes)
}
