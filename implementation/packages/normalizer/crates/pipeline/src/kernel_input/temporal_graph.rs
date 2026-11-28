//! One identity and publication graph for independent member choices.
use super::*;
use super::member_choice::{Candidates, Fallback, Occurrence};
use crate::formatting::sql::scoped_relation::{ChoiceOuter, ChoiceShape, Operand, Slot};

#[derive(PartialEq, Eq)]
pub(super) enum Definition {
    Choice(Occurrence),
}

impl Definition {
    fn operand(&self) -> Option<&Expr> {
        match self {
            Self::Choice(node) => match &node.empty {
                Fallback::Integer(value) => Some(value),
                Fallback::RecordParameter {..} => None,
            },
        }
    }

    fn is_total(&self) -> bool {
        match self {
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

/// Supported computed operands are already acquired exact integers.
fn acquired_integer_math(value: &Expr, domains: &BTreeMap<String, String>, names: &BTreeMap<String, String>) -> bool {
    if value.as_lit().and_then(math::Lit::as_integer).is_some() { return true; }
    if let Some(SymVarOrigin::Parameter { name, primitive_type }) = value.as_sym_var() {
        return primitive_type == "integer" && domains.get(name).is_some_and(|kind| kind == "number")
            && names.contains_key(name);
    }
    if let Some((math::UnaryOpKind::Neg, operand)) = value.as_unary_op() {
        return acquired_integer_math(operand, domains, names);
    }
    value.as_binop().is_some_and(|(op, left, right)| {
        matches!(op, BinOpKind::Add | BinOpKind::Sub | BinOpKind::Mul)
            && acquired_integer_math(left, domains, names) && acquired_integer_math(right, domains, names)
    })
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
        let mut dependencies = Vec::new();
        let Definition::Choice(choice) = node;
        if let Some(operand) = node.operand() {
            operand.walk(&mut |part| dependencies.extend(integer_definition_id(part)));
        }
        choice.guard.walk(&mut |part| dependencies.extend(integer_definition_id(part)));
        for dep in dependencies {
            need(dep < id, "temporal dependency must already be defined")?;
            if pending.insert(dep) { inspect.push(dep); }
        }
    }
    let mut published = Publication::default();
    let array_name = |name: &str| names.get(name).filter(|_| arrays.contains(name))
        .ok_or_else(|| fail("temporal source requires a published array parameter"));
    // Dependency validation above proves that ascending acquisition order
    // is topological. These handles are only for wiring rendering, not a
    // second structural ordering for values.
    for id in pending {
        let node = &nodes[id as usize];
        let value = if let Definition::Choice(Occurrence {empty: Fallback::RecordParameter {owner, name}, ..}) = node {
            need(domains.get(name) == Some(owner), "record choice fallback needs its published record parameter")?;
            Operand::Parameter(names.get(name).cloned().ok_or_else(|| fail("invalid record fallback name"))?)
        } else {
            let operand = node.operand().ok_or_else(|| fail("missing scalar temporal operand"))?;
            if let Some(dep) = integer_definition_id(operand) {
                let &slot = published.aliases.get(&dep).ok_or_else(|| fail("relation dependency was not published"))?;
                let Definition::Choice(current) = node;
                let guard = &current.guard;
                let prior_guard = match &nodes[dep as usize] {
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
                let Definition::Choice(current) = node;
                let mut seen = BTreeSet::new();
                let mut well_bound = true;
                operand.walk(&mut |part| {
                    if let Some(dep) = integer_definition_id(part) {
                        if let (Some(&slot), Some(Definition::Choice(prior))) = (published.aliases.get(&dep), nodes.get(dep as usize)) {
                            if b::path_implies(&current.guard, &prior.guard) { seen.insert(slot); }
                            else { well_bound = false; }
                        } else { well_bound = false; }
                    }
                });
                let closed_sum = {
                    let mut closed = well_bound;
                    operand.walk(&mut |part| {
                        closed &= if let Some((kind, relation, _, _)) = part.as_relation_aggregate() {
                            kind == math::RelationAggregateKind::Sum &&
                                matches!(relation, RelationExpr::Base { name, alias, snapshot: None }
                                    if arrays.contains(name) && alias == "value")
                        } else if let Some(row) = part.as_row_value() { row == "value" }
                        else if let Some(SymVarOrigin::External { binding_id, path, primitive_type }) = part.as_sym_var() {
                            primitive_type == "integer" && acquired_path(*binding_id, path).is_some()
                        } else {
                            integer_definition_id(part).is_some() || acquired_integer_math(part, domains, names)
                            || part.as_binop().is_some_and(|(op, _, _)| matches!(op, BinOpKind::Add | BinOpKind::Sub | BinOpKind::Mul))
                        }
                    });
                    closed
                };
                need(closed_sum, "member choice fallback must be an acquired integer expression")?;
                Operand::Computed { value: operand.clone(), dependencies: seen.into_iter().collect() }
            }
        };
        let slot = published.aliases.len();
        let row = match &nodes[id as usize] {
            Definition::Choice(choice) => {
                let mut entry_guard = true;
                choice.guard.walk(&mut |node| {
                    entry_guard &= node.as_lit().is_some() || node.as_binop().is_some() || node.as_unary_op().is_some()
                        || matches!(node.as_sym_var(), Some(SymVarOrigin::Parameter { name, primitive_type })
                            if matches!(primitive_type.as_str(), "integer" | "boolean") && names.contains_key(name))
                        || integer_definition_id(node).is_some_and(|dep|
                            dep < id && nodes.get(dep as usize).is_some_and(Definition::is_total));
                });
                need(entry_guard, "member choice guard requires acquired scalars or an earlier total lexical value")?;
                let members = match &choice.members {
                    Candidates::Input(name) => match &choice.empty {
                        Fallback::Integer(_) => array_name(name)?.clone(),
                        Fallback::RecordParameter {owner, ..} => {
                            need(domains.get(name).is_some_and(|domain| domain == &format!("{owner}[]")),
                                "record choice needs its published record array parameter")?;
                            names.get(name).cloned().ok_or_else(|| fail("invalid record array name"))?
                        }
                    },
                    Candidates::Literal(values) => format!("[{}]", values.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ")),
                };
                let (fallback, element) = match &choice.empty {
                    Fallback::Integer(value) => (ChoiceOuter::Expression(value.clone()), "number"),
                    Fallback::RecordParameter { owner, name } => (
                        ChoiceOuter::Parameter { name: name.clone(), kind: owner.clone() }, owner.as_str()),
                };
                let source = match &choice.members {
                    Candidates::Input(name) => ChoiceOuter::Parameter { name: name.clone(), kind: format!("{element}[]") },
                    Candidates::Literal(values) => ChoiceOuter::Expression(b::array_lit(values.iter()
                        .cloned().map(b::lit_integer).collect())),
                };
                Slot::Choice { members, empty: value, guard: choice.guard.clone(), shape: ChoiceShape { source, fallback } }
            }
        };
        published.aliases.insert(id, slot);
        if nodes[id as usize].is_total() { published.total.insert(id); }
        published.slots.push(row);
    }
    Ok(published)
}

/// Join mutually exclusive, unobserved choices with exactly the same
/// support and empty value on their respective domains. This is not reuse
/// of two choices that could both occur.
pub(super) fn same_support_choice(
    value: &Expr, definitions: &mut Vec<Definition>,
) -> Option<Expr> {
    let ([(condition, selected)], otherwise) = value.as_total_case_when()? else { return None; };
    let left = integer_definition_id(selected)?;
    let right = integer_definition_id(otherwise)?;
    if left == right { return None; }
    let Definition::Choice(left) = definitions.get(left as usize)?;
    let Definition::Choice(right) = definitions.get(right as usize)?;
    if left.members != right.members { return None; }
    let (Fallback::Integer(left_empty), Fallback::Integer(right_empty)) = (&left.empty, &right.empty)
    else { return None; };
    // The guard and both alternative fallbacks must use entry values only,
    // never an observation or another choice in a different scope.
    fn closed(value: &Expr) -> bool {
        let mut proved = true;
        value.walk(&mut |node| {
            proved &= node.as_lit().is_some() || node.as_binop().is_some() || node.as_unary_op().is_some()
                || matches!(node.as_sym_var(), Some(SymVarOrigin::Parameter { name, primitive_type })
                    if matches!(primitive_type.as_str(), "integer" | "boolean")
                        && !name.starts_with('$') && !name.contains(['.', '[', ']']));
        });
        proved
    }
    if !closed(condition) || !closed(left_empty) || !closed(right_empty) { return None; }
    let complement = math::normalize(b::not(condition.clone()));
    if !b::path_implies(condition, &left.guard) || !b::path_implies(&left.guard, condition)
        || !b::path_implies(&complement, &right.guard) || !b::path_implies(&right.guard, &complement)
    { return None; }
    let same_value = b::eq(left_empty.clone(), right_empty.clone());
    let empty = if b::path_implies(condition, &same_value) { right_empty.clone() }
        else if b::path_implies(&complement, &same_value) { left_empty.clone() }
        else { return None; };
    let members = left.members.clone();
    let id = definitions.len();
    definitions.push(Definition::Choice(Occurrence {
        members, empty: Fallback::Integer(empty), guard: b::lit_bool(true),
    }));
    Some(b::integer_temporal_result(id as u64))
}

/// Prove every partial reference is reached only after its occurrence.
pub(super) fn validate_reads(expression: &Expr, nodes: &[Definition]) -> Result<()> {
    fn check(expression: &Expr, active: &Expr, nodes: &[Definition]) -> Result<()> {
        if let Some(id) = temporal_definition_id(expression) {
            let def = nodes.get(id as usize).ok_or_else(|| fail("unknown temporal reference"))?;
            return match def {
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
        } else if expression.as_relation_aggregate().is_some() {
            let mut result = Ok(());
            expression.walk(&mut |part| {
                if result.is_ok() && temporal_definition_id(part).is_some() {
                    result = check(part, active, nodes);
                }
            });
            result?;
        } else if let Some(args) = expression.as_array_lit().or_else(|| expression.as_call().map(|(_, args)| args)) {
            for arg in args { check(arg, active, nodes)?; }
        }
        Ok(())
    }
    check(expression, &b::lit_bool(true), nodes)
}
