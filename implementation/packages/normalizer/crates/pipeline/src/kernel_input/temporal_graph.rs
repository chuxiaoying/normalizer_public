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

type TableColumns = BTreeMap<String, BTreeMap<String, String>>;

#[derive(Clone, Copy)]
enum RowSlot<'a> {
    Scalar(&'a str),
    Table { alias: &'a str, columns: &'a BTreeMap<String, String> },
}

impl RowSlot<'_> {
    fn binds(&self, field: &str) -> bool {
        match self {
            Self::Scalar(alias) => field == *alias,
            Self::Table { alias, columns } => field.split_once('.')
                .is_some_and(|(owner, field)| owner == *alias && columns.contains_key(field)),
        }
    }
}

// Every relation contributing to an acquired fallback has its own row
// scope. A physical table denotes a particular already acquired snapshot.
fn aggregate_row<'a>(
    relation: &'a RelationExpr, arrays: &BTreeSet<String>, tables: &'a TableColumns,
    acquired_table: &impl Fn(&str, u64) -> bool,
) -> Option<RowSlot<'a>> {
    use math::domains::relation::positional::integer_domain;
    if let Some((_, alias)) = integer_domain(relation) { return Some(RowSlot::Scalar(alias)); }
    let mut source = relation;
    while let RelationExpr::Select { input, .. } = source { source = input; }
    match source {
        RelationExpr::Base { name, alias, snapshot: Some(snapshot) } if acquired_table(name, *snapshot) =>
            Some(RowSlot::Table { alias, columns: tables.get(name)? }),
        RelationExpr::Base { name, alias, snapshot: None } if alias == "value" && arrays.contains(name) =>
            Some(RowSlot::Scalar(alias)),
        // An initial unconditional table read has no event handle. The
        // table/column declarations describe that entry snapshot directly.
        RelationExpr::Base { name, alias, snapshot: None } if alias == name =>
            Some(RowSlot::Table { alias, columns: tables.get(name)? }),
        _ => None,
    }
}

fn acquired_columns<'a>(
    table: &math::domains::relation::TableSource, tables: &'a TableColumns,
    acquired: &impl Fn(&str, u64) -> bool,
) -> Option<&'a BTreeMap<String, String>> {
    if table.snapshot.is_some_and(|snapshot| !acquired(&table.name, snapshot)) { return None; }
    tables.get(&table.name)
}

fn closed_computed_value(
    value: &Expr, rows: &[RowSlot<'_>], arrays: &BTreeSet<String>, tables: &TableColumns,
    acquired: &impl Fn(&Expr) -> bool, acquired_table: &impl Fn(&str, u64) -> bool,
) -> bool {
    let mut closed = true;
    value.walk_pruned(&mut |part| {
        if !closed { return false; }
        if let Some((kind, relation, select, fallback)) = part.as_relation_aggregate() {
            use math::RelationAggregateKind::{Count, FirstValue, NthValue, Sum};
            if kind == Count
                && matches!(relation, RelationExpr::Base { name, alias, snapshot: None }
                    if arrays.contains(name) && alias == "value")
                && select.is_none() && fallback.is_none()
            {
                return false;
            }
            // A positional operand keeps its index in the enclosing scope.
            // Its absence is handled by the surrounding fallback CASE.
            if kind == NthValue {
                closed = matches!(relation, RelationExpr::Base { name, alias, snapshot: None }
                    if arrays.contains(name) && alias == "value")
                    && fallback.is_none()
                    && select.is_some_and(|index|
                        closed_computed_value(index, rows, arrays, tables, acquired, acquired_table));
                return false;
            }
            let counts_rows = kind == Sum && select == Some(&b::lit_int(1)) && fallback.is_none();
            match relation {
                RelationExpr::Grouped(group) => {
                    closed = counts_rows && acquired_columns(group.table(), tables, acquired_table)
                        .is_some_and(|columns| group.columns().all(|name| columns.contains_key(name)));
                    return false;
                }
                RelationExpr::Join { left, right, on, filters } => {
                    let mut scoped = rows.to_vec();
                    for (table, alias) in [left, right] {
                        let Some(columns) = acquired_columns(table, tables, acquired_table) else {
                            closed = false;
                            return false;
                        };
                        scoped.push(RowSlot::Table { alias, columns });
                    }
                    closed = counts_rows && std::iter::once(on).chain(filters).all(|value|
                        closed_computed_value(value, &scoped, arrays, tables, acquired, acquired_table));
                    return false;
                }
                RelationExpr::ProjectedUnion(branches) => {
                    closed = kind == Count && select.is_none() && fallback.is_none() &&
                        branches.iter().all(|branch| {
                            let Some(columns) = acquired_columns(&branch.table, tables, acquired_table) else { return false; };
                            if !columns.contains_key(&branch.column) { return false; }
                            let mut scoped = rows.to_vec();
                            scoped.push(RowSlot::Table { alias: &branch.table.name, columns });
                            branch.filters.iter().all(|value|
                                closed_computed_value(value, &scoped, arrays, tables, acquired, acquired_table))
                        });
                    return false;
                }
                _ => {}
            }
            let source_relation = match relation {
                RelationExpr::Distinct(input) if kind == Count => input,
                other => other,
            };
            let (source_relation, ordering) = math::domains::relation::order_components(source_relation)
                .unwrap_or((source_relation, &[]));
            let supported = matches!(kind, Sum | FirstValue | Count);
            let Some(row) = supported.then(|| aggregate_row(source_relation, arrays, tables, acquired_table)).flatten() else {
                closed = false;
                return false;
            };
            let mut scoped = rows.to_vec();
            scoped.push(row);
            let mut source = source_relation;
            while let RelationExpr::Select { predicate, input } = source {
                closed &= closed_computed_value(predicate, &scoped, arrays, tables, acquired, acquired_table);
                source = input;
            }
            if let RelationExpr::Vector { value, .. } = source {
                // Bounds live in the enclosing scope, not the row scope.
                closed &= closed_computed_value(value, rows, arrays, tables, acquired, acquired_table);
            }
            closed &= select.is_some_and(|value| closed_computed_value(value, &scoped, arrays, tables, acquired, acquired_table));
            closed &= ordering.iter().all(|key|
                closed_computed_value(key, &scoped, arrays, tables, acquired, acquired_table));
            // Only the projection, filters and order keys see the current row. A FIRST
            // default follows the same outer-scope rule as its interval.
            closed &= match fallback {
                Some(value) if kind == FirstValue => closed_computed_value(value, rows, arrays, tables, acquired, acquired_table),
                None => kind != FirstValue,
                _ => false,
            };
            return false;
        }
        use math::domains::relation::positional::{minimum_bounds, range_bounds};
        closed = if let Some(row) = part.as_row_value() { rows.iter().rev().any(|slot| slot.binds(row)) }
            else {
                acquired(part)
                || part.as_lit().is_some_and(|lit| lit.as_integer().is_some() || matches!(lit, math::Lit::Bool(_) | math::Lit::Str(_) | math::Lit::StrUnits(_) | math::Lit::Null))
                || part.as_binop().is_some()
                || matches!(part.as_unary_op(), Some((math::UnaryOpKind::Neg | math::UnaryOpKind::Not, _)))
                || part.as_total_case_when().is_some()
                || range_bounds(part).is_some() || minimum_bounds(part).is_some()
                || math::domains::integer_quotient::is_quotient(part)
            };
        closed
    });
    closed
}

pub(super) fn publish(
    nodes: &[Definition], expression: Option<&Expr>, arrays: &BTreeSet<String>,
    domains: &BTreeMap<String, String>, names: &BTreeMap<String, String>,
    acquired_path: impl Fn(u64, &[String]) -> Option<String>,
    tables: &TableColumns, acquired_table: impl Fn(&str, u64, &Expr) -> bool,
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
                let acquired = |part: &Expr| match part.as_sym_var() {
                    Some(SymVarOrigin::External { binding_id, path, primitive_type }) =>
                        matches!(primitive_type.as_str(), "integer" | "boolean" | "text" | "text?")
                            && acquired_path(*binding_id, path).is_some(),
                    Some(SymVarOrigin::Parameter {name, primitive_type}) => {
                        let domain = match primitive_type.as_str() {
                            "integer" => "number", "boolean" => "bool",
                            "text" => "text", "text?" => "text?", _ => return false,
                        };
                        domains.get(name).is_some_and(|kind| kind == domain) && names.contains_key(name)
                    }
                    _ => integer_definition_id(part).is_some(),
                };
                let snapshot_available = |table: &str, snapshot| acquired_table(table, snapshot, &current.guard);
                need(well_bound && closed_computed_value(operand, &[], arrays, tables, &acquired, &snapshot_available),
                    "member choice fallback must be an acquired integer expression")?;
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
