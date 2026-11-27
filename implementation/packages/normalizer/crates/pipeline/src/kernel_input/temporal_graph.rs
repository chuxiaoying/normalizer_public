//! One identity and publication graph for deterministic folds and independent choices.
use super::*;
use super::member_choice::{Candidates, Fallback, Occurrence};
use crate::formatting::sql::scoped_relation::{Operand, Slot};

#[derive(PartialEq, Eq)]
pub(super) enum Definition {
    Fold(super::pure_fold::Definition),
    Choice(Occurrence),
}

impl Definition {
    fn operand(&self) -> Option<&Expr> {
        match self {
            Self::Fold(node) => Some(&node.initial),
            Self::Choice(node) => match &node.empty {
                Fallback::Integer(value) => Some(value),
                Fallback::RecordParameter {..} => None,
            },
        }
    }

    fn is_total(&self) -> bool {
        match self {
            Self::Fold(node) => node.guard.as_bool() == Some(true),
            Self::Choice(node) => node.guard.as_bool() == Some(true),
        }
    }
}

#[derive(Default)]
pub(super) struct Publication {
    pub(super) aliases: BTreeMap<u64, usize>,
    pub(super) total: BTreeSet<u64>,
    pub(super) slots: Vec<Slot>,
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum Ready {
    Fold(String, Operand, std::rc::Rc<math::CanonicalKey>),
    Choice(u64),
}

pub(super) fn publish(
    nodes: &[Definition], expression: Option<&Expr>, arrays: &BTreeSet<String>,
    domains: &BTreeMap<String, String>, names: &BTreeMap<String, String>,
    acquired_path: impl Fn(u64, &[String]) -> Option<String>,
) -> Result<Publication> {
    let mut pending = BTreeSet::new();
    if let Some(expression) = expression {
        expression.walk(&mut |node| { pending.extend(temporal_definition_id(node)); });
    }
    let mut inspect: Vec<_> = pending.iter().copied().collect();
    while let Some(id) = inspect.pop() {
        let node = nodes.get(id as usize).ok_or_else(|| fail("undefined temporal definition"))?;
        if let Some(dep) = node.operand().and_then(integer_definition_id) {
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
            let value = if let Definition::Choice(Occurrence {empty: Fallback::RecordParameter {owner, name}, ..}) = node {
                need(domains.get(name) == Some(owner), "record choice fallback needs its published record parameter")?;
                Operand::Parameter(names.get(name).cloned().ok_or_else(|| fail("invalid record fallback name"))?)
            } else {
                let operand = node.operand().ok_or_else(|| fail("missing scalar temporal operand"))?;
                if let Some(dep) = integer_definition_id(operand) {
                    let Some(&slot) = published.aliases.get(&dep) else { continue; };
                    let guard = match node { Definition::Fold(fold) => &fold.guard, Definition::Choice(choice) => &choice.guard };
                    let prior_guard = match &nodes[dep as usize] {
                        Definition::Fold(fold) => &fold.guard,
                        Definition::Choice(choice) => &choice.guard,
                    };
                    need(b::path_implies(guard, prior_guard),
                        "relation operand is not available on its path")?;
                    Operand::Prior(slot)
                } else if let Some(n) = operand.as_lit().and_then(math::Lit::as_integer) {
                    Operand::Integer(n)
                } else if let Some(SymVarOrigin::Parameter { name, .. }) = operand.as_sym_var() {
                    // Fold initials and numeric choice fallbacks admit an
                    // exact integer before entering the temporal graph.
                    need(domains.get(name).is_some_and(|domain| domain == "number"),
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
                }
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
        let slot = published.aliases.len();
        let row = match &nodes[id as usize] {
            Definition::Fold(fold) => {
                let mut ctx = crate::FormatCtx::for_business(BTreeMap::new(), BTreeMap::new());
                let body = crate::format_expr_with_ctx(&fold.body, &mut ctx);
                Slot::Fold { source: array_name(&fold.source)?.clone(), initial: value, body }
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
                    Candidates::Input(name) => match &choice.empty {
                        Fallback::Integer(_) => array_name(name)?.clone(),
                        Fallback::RecordParameter {owner, ..} => {
                            need(domains.get(name).is_some_and(|domain| domain == &format!("{owner}[]")),
                                "record choice needs its published record array parameter")?;
                            names.get(name).cloned().ok_or_else(|| fail("invalid record array name"))?
                        }
                    },
                    Candidates::Literal(values) => format!("[{}]", values.iter().map(i32::to_string).collect::<Vec<_>>().join(", ")),
                };
                Slot::Choice { members, empty: value, guard: choice.guard.clone() }
            }
        };
        published.aliases.insert(id, slot);
        if nodes[id as usize].is_total() { published.total.insert(id); }
        published.slots.push(row);
        pending.remove(&id);
    }
    Ok(published)
}

/// Prove every partial reference is reached only after its occurrence.
pub(super) fn validate_reads(expression: &Expr, nodes: &[Definition]) -> Result<()> {
    fn check(expression: &Expr, active: &Expr, nodes: &[Definition]) -> Result<()> {
        if let Some(id) = temporal_definition_id(expression) {
            let def = nodes.get(id as usize).ok_or_else(|| fail("unknown temporal reference"))?;
            return match def {
                Definition::Fold(fold) => {
                    need(b::path_implies(active, &fold.guard), "fold use outside its source path")?;
                    check(&fold.initial, active, nodes)
                },
                Definition::Choice(choice) => {
                    need(b::path_implies(active, &choice.guard),
                        "member choice read outside its acquisition guard")?;
                    if let Fallback::Integer(value) = &choice.empty {
                        check(value, &choice.guard, nodes)?;
                    }
                    Ok(())
                },
            };
        }
        if let Some(record) = record_choice_projection(expression) {
            return check(record, active, nodes);
        }
        if let Some(branches) = expression.as_case_when() {
            let mut remaining = active.clone();
            for (test, value) in branches {
                check(test, &remaining, nodes)?;
                let (selected, later) = b::first_match_domains(remaining, test);
                check(value, &selected, nodes)?;
                remaining = later;
            }
        } else if let Some((_, left, right)) = expression.as_binop() {
            check(left, active, nodes)?;
            check(right, active, nodes)?;
        } else if let Some((_, operand)) = expression.as_unary_op() {
            check(operand, active, nodes)?;
        } else if let Some(args) = expression.as_array_lit().or_else(|| expression.as_call().map(|(_, args)| args)) {
            for arg in args { check(arg, active, nodes)?; }
        }
        Ok(())
    }
    check(expression, &b::lit_bool(true), nodes)
}
