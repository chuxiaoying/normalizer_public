//! CF-02: structural keys for already bound IR. Constructing a key does not
//! prove equivalence or normalize a child. Callers prove movement separately.
use crate::domains::{decimal, primitive::registry, relation::{RelationExpr, TableSource}};
use crate::expr::{BinOpKind, Expr, ExprKind, RelationAggregateKind, UnaryOpKind};
use crate::sort_context::SortContext;
use crate::symvar::SymVarOrigin;
use num_bigint::{BigInt, BigUint, Sign};
use std::{cell::RefCell, cmp::Ordering, collections::HashMap, rc::Rc};

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Class { Reference, Read, Scalar, Relation, Construct, Literal }

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Operation { Eq, Ne, Gt, Lt, Ge, Le, And, Or, Not, Named(String) }

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Payload {
    Bool(bool),
    Number(Exact),
    Text(String),
    Texts(Vec<String>),
    Table(TableKey),
    Tables(Vec<(TableKey, String)>),
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct TableKey { path: Vec<String>, snapshot: Option<Observation> }

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Observation { Entry(Vec<String>), Receipt(u64), Runtime(u64), Unnamed(u64) }

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Identity {
    Column(Vec<String>),
    EntrySnapshot(Vec<String>),
    Local(usize),
    Parameter(Vec<String>),
    Receipt(u64, Vec<String>),
    RuntimeObservation(u64, Vec<String>),
    // These distinguish in-flight execution values until the result builder
    // supplies their business identity. They are not published reference names.
    InternalExternal(u64, Vec<String>),
    InternalHeap(i64),
    InternalTemporal(usize),
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CanonicalKey {
    arms: BigUint,
    operations: BigUint,
    class: Class,
    operation: Operation,
    types: Vec<String>,
    contract: Vec<String>,
    payload: Vec<Payload>,
    children: Vec<Option<Rc<CanonicalKey>>>,
    tail: Option<Rc<CanonicalKey>>,
    identity: Option<Identity>,
}

#[derive(Clone)]
struct Exact(BigInt, u32);
impl Ord for Exact {
    fn cmp(&self, other: &Self) -> Ordering {
        let (a, b) = (&self.0, &other.0);
        a.sign().cmp(&b.sign()).then_with(|| {
            if a.sign() == Sign::NoSign { return Ordering::Equal; }
            let (x, y) = (a.magnitude().to_str_radix(10), b.magnitude().to_str_radix(10));
            let cmp = (x.len() as i128 - i128::from(self.1))
                .cmp(&(y.len() as i128 - i128::from(other.1)))
                .then_with(|| (0..x.len().max(y.len())).find_map(|i| {
                    let digit = |s: &str| s.as_bytes().get(i).copied().unwrap_or(b'0');
                    let result = digit(&x).cmp(&digit(&y));
                    (!result.is_eq()).then_some(result)
                }).unwrap_or(Ordering::Equal));
            if a.sign() == Sign::Minus { cmp.reverse() } else { cmp }
        })
    }
}
impl PartialOrd for Exact { fn partial_cmp(&self, other: &Self) -> Option<Ordering> { Some(self.cmp(other)) } }
impl PartialEq for Exact { fn eq(&self, other: &Self) -> bool { self.cmp(other).is_eq() } }
impl Eq for Exact {}

fn named(name: &str) -> Operation { Operation::Named(name.into()) }
fn path(name: &str) -> Vec<String> { name.split('.').map(str::to_owned).collect() }

fn node(
    class: Class, operation: Operation, types: Vec<String>, contract: Vec<String>,
    payload: Vec<Payload>, children: Vec<Option<Rc<CanonicalKey>>>,
    identity: Option<Identity>, arms: usize, operations: usize,
) -> Rc<CanonicalKey> {
    let mut arms = BigUint::from(arms);
    let mut operations = BigUint::from(operations);
    for child in children.iter().flatten() {
        arms += &child.arms;
        operations += &child.operations;
    }
    Rc::new(CanonicalKey { arms, operations, class, operation, types, contract, payload, children, tail: None, identity })
}

fn bin_op(op: BinOpKind) -> Operation {
    match op {
        BinOpKind::Eq => Operation::Eq, BinOpKind::Ne => Operation::Ne,
        BinOpKind::Gt => Operation::Gt, BinOpKind::Lt => Operation::Lt,
        BinOpKind::Ge => Operation::Ge, BinOpKind::Le => Operation::Le,
        BinOpKind::And => Operation::And, BinOpKind::Or => Operation::Or,
        BinOpKind::Add => named("add"), BinOpKind::Mul => named("mul"), BinOpKind::Sub => named("sub"),
    }
}

fn aggregate(kind: RelationAggregateKind) -> (&'static str, &'static str) {
    match kind {
        RelationAggregateKind::Count => ("count", "number"),
        RelationAggregateKind::Sum => ("sum", "number"),
        RelationAggregateKind::FirstValue => ("first", "member"),
        RelationAggregateKind::NthValue => ("nth", "member"),
        RelationAggregateKind::Exists => ("exists", "bool"),
    }
}

fn observation(ctx: &dyn SortContext, id: u64) -> Observation {
    match ctx.external_names().and_then(|names| names.get(&id)) {
        Some(name) => {
            let suffix = |prefix| name.strip_prefix(prefix).and_then(|value| value.parse::<u64>().ok());
            if let Some(slot) = suffix("$e_") { Observation::Receipt(slot) }
            else if let Some(slot) = suffix("$a_") { Observation::Runtime(slot) }
            else { Observation::Entry(path(name)) }
        }
        None => Observation::Unnamed(id),
    }
}

fn physical(table: &TableSource, ctx: &dyn SortContext) -> TableKey {
    TableKey { path: path(&table.name), snapshot: table.snapshot.map(|id| observation(ctx, id)) }
}

fn literal(ty: &str, payload: Vec<Payload>) -> Rc<CanonicalKey> {
    node(Class::Literal, named("literal"), vec![ty.into()], vec![], payload, vec![], None, 0, 0)
}

fn binding(class: Class, ty: &str, id: Identity) -> Rc<CanonicalKey> {
    let read = usize::from(class == Class::Read);
    node(class, named(if read == 0 { "binding" } else { "path" }), vec![ty.into()], vec![], vec![], vec![], Some(id), 0, read)
}

struct Keys<'a> {
    context: &'a dyn SortContext,
    memo: HashMap<Expr, Rc<CanonicalKey>>,
    in_reduce_body: bool,
}

impl Keys<'_> {
    fn expr(&mut self, expression: &Expr) -> Rc<CanonicalKey> {
        if let Some(key) = self.memo.get(expression) { return key.clone(); }
        let key = self.expr_uncached(expression);
        self.memo.insert(expression.clone(), key.clone());
        key
    }

    fn expr_uncached(&mut self, expression: &Expr) -> Rc<CanonicalKey> {
        use ExprKind::*;
        let ctx = self.context;
        if let Some(decimal::View::Literal(coefficient, scale)) = decimal::view(expression) {
            return literal("number", vec![Payload::Number(Exact(coefficient, scale))]);
        }
        match expression.kind() {
            Lit(value) => match value {
                crate::Lit::Int(_) | crate::Lit::BigInt(_) =>
                    literal("number", vec![Payload::Number(Exact(value.as_integer().expect("integer literal"), 0))]),
                crate::Lit::Bool(value) => literal("bool", vec![Payload::Bool(*value)]),
                crate::Lit::Str(value) => literal("text", vec![Payload::Text(value.clone())]),
                crate::Lit::Null => literal("null", vec![]),
            },
            SymVar { origin } => {
                let (class, id) = match origin {
                    SymVarOrigin::Parameter { name, primitive_type: _ } => {
                        let local = self.in_reduce_body.then(|| crate::ReduceBodyContext::resolve(name)).flatten();
                        let class = if local.is_none() && name.contains('.') { Class::Read } else { Class::Reference };
                        (class, local.map_or_else(|| Identity::Parameter(path(name)), |(slot, _)| Identity::Local(slot)))
                    },
                    SymVarOrigin::External { binding_id, path: parts, primitive_type: _ } => {
                        let id = match observation(ctx, *binding_id) {
                            Observation::Entry(mut root) => { root.extend(parts.iter().cloned()); Identity::EntrySnapshot(root) }
                            Observation::Receipt(slot) => Identity::Receipt(slot, parts.clone()),
                            Observation::Runtime(slot) => Identity::RuntimeObservation(slot, parts.clone()),
                            Observation::Unnamed(id) => Identity::InternalExternal(id, parts.clone()),
                        };
                        (if parts.is_empty() { Class::Reference } else { Class::Read }, id)
                    }
                    SymVarOrigin::Temporal { binding_id, primitive_type: _ } => {
                        if let Some(reduce) = ctx.reduce_shapes().and_then(|shapes| shapes.get(binding_id)) {
                            let source = binding(Class::Reference, "number[]",
                                Identity::Parameter(path(&reduce.source)));
                            let initial = self.expr(&reduce.initial);
                            let mut scope = Keys { context: ctx, memo: HashMap::new(), in_reduce_body: true };
                            let body = scope.expr(&reduce.body);
                            return node(Class::Relation, named("reduce"), vec!["number".into()],
                                vec![], vec![], vec![Some(source), Some(initial), Some(body)], None, 0, 1);
                        }
                        if let Some(&slot) = ctx.choice_slots().and_then(|slots| slots.get(binding_id)) {
                            return binding(Class::Reference, origin.primitive_type(), Identity::Local(slot));
                        }
                        let slot = ctx.temporal_aliases().and_then(|aliases| aliases.get(binding_id)).copied()
                            .unwrap_or_else(|| usize::try_from(*binding_id).expect("local identity does not fit its slot"));
                        (Class::Reference, Identity::InternalTemporal(slot))
                    }
                };
                binding(class, origin.primitive_type(), id)
            }
            HeapRef(id) => binding(Class::Reference, "reference", ctx.heap_display_name(*id)
                .map_or(Identity::InternalHeap(*id), |name| Identity::Parameter(path(&name)))),
            RowValue { alias, acquired_integer } => binding(if alias.contains('.') { Class::Read } else { Class::Reference },
                if *acquired_integer { "integer" } else { "row_value" }, Identity::Column(path(alias))),
            Call { key, args } => {
                let (name, parameters, result) = registry::func_signature(*key);
                let ty = result.map(|result| result.algebra_name().into()).into_iter().collect();
                let signature = parameters.iter().map(|p| p.algebra_name().into()).collect();
                let class = if crate::domains::composite::field_intrinsic_result(*key).is_some()
                    || crate::domains::composite::vector_length_input(expression).is_some()
                {
                    Class::Read
                } else if crate::domains::composite::ordered_text_map_entries(expression).is_some() {
                    Class::Construct
                } else {
                    Class::Scalar // Other registered Call nodes are scalar operations.
                };
                let args = args.iter().map(|arg| Some(self.expr(arg))).collect();
                node(class, Operation::Named(name), ty, signature, vec![], args, None, 0, 1)
            }
            BinOp { op, left, right } => {
                let types = match op {
                    BinOpKind::Eq | BinOpKind::Ne | BinOpKind::Gt | BinOpKind::Lt | BinOpKind::Ge | BinOpKind::Le
                        | BinOpKind::And | BinOpKind::Or => vec!["bool".into()],
                    BinOpKind::Add | BinOpKind::Sub | BinOpKind::Mul => vec![],
                };
                node(Class::Scalar, bin_op(*op), types, vec![], vec![],
                    vec![Some(self.expr(left)), Some(self.expr(right))], None, 0, 1)
            }
            UnaryOp { op, operand } => {
                let name = match op { UnaryOpKind::Not => Operation::Not, UnaryOpKind::Neg => named("neg") };
                node(Class::Scalar, name, vec![], vec![], vec![], vec![Some(self.expr(operand))], None, 0, 1)
            }
            CaseWhen { branches } => {
                let mut slots = Vec::new();
                let terminal = branches.last().is_some_and(|(guard, _)| guard.as_bool() == Some(true));
                let decision_count = branches.len() - usize::from(terminal);
                for (guard, value) in &branches[..decision_count] {
                    slots.extend([Some(self.expr(guard)), Some(self.expr(value))]);
                }
                let mut case = node(Class::Construct, named("case"), vec![], vec![], vec![], slots,
                    None, branches.len(), 1);
                if terminal {
                    let fallback = self.expr(&branches[decision_count].1);
                    let key = Rc::get_mut(&mut case).expect("new CASE key");
                    key.arms += &fallback.arms;
                    key.operations += &fallback.operations;
                    key.tail = Some(fallback);
                }
                case
            }
            ArrayLit(values) => {
                let children = values.iter().map(|value| Some(self.expr(value))).collect();
                node(Class::Construct, named("sequence"), vec![], vec![], vec![], children, None, 0, 1)
            }
            Relation(relation) => self.relation(relation),
            RelationAggregate { kind, relation, select_expr, default_expr } => {
                let (operation, ty) = aggregate(*kind);
                let children = vec![Some(self.relation(relation)),
                    select_expr.as_ref().map(|child| self.expr(child)),
                    default_expr.as_ref().map(|child| self.expr(child))];
                node(Class::Relation, named(operation), vec![ty.into()], vec![], vec![], children, None, 0, 1)
            }
        }
    }

    fn relation(&mut self, relation: &RelationExpr) -> Rc<CanonicalKey> {
        let (operation, payload, children) = match relation {
            RelationExpr::Empty => ("empty", vec![], vec![]),
            RelationExpr::Base { name, alias, snapshot } => ("base", vec![
                Payload::Table(TableKey { path: path(name), snapshot: snapshot.map(|id| observation(self.context, id)) }),
                Payload::Text(alias.clone()),
            ], vec![]),
            RelationExpr::Vector { value, alias } =>
                ("vector", vec![Payload::Text(alias.clone())], vec![Some(self.expr(value))]),
            RelationExpr::Select { predicate, input } =>
                ("select", vec![], vec![Some(self.relation(input)), Some(self.expr(predicate))]),
            RelationExpr::Project { value, alias, input } =>
                ("project", vec![Payload::Text(alias.clone())], vec![Some(self.relation(input)), Some(self.expr(value))]),
            RelationExpr::Distinct(input) => ("distinct", vec![], vec![Some(self.relation(input))]),
            RelationExpr::ProjectedUnion(columns) => {
                let mut sources: Vec<_> = columns.iter().map(|(table, column)| (physical(table, self.context), column.to_owned())).collect();
                sources.sort();
                ("projected_union", vec![Payload::Tables(sources)], vec![])
            }
            RelationExpr::Grouped(group) => ("group", vec![
                Payload::Table(physical(group.table(), self.context)), Payload::Texts(group.columns().map(str::to_owned).collect()),
            ], vec![]),
            RelationExpr::RecordProjection { source, field } =>
                ("record_projection", vec![Payload::Text(source.clone()), Payload::Text(field.clone())], vec![]),
            RelationExpr::Join { left, right, on, filters } => {
                let sides = vec![(physical(&left.0, self.context), left.1.clone()), (physical(&right.0, self.context), right.1.clone())];
                let children = std::iter::once(on).chain(filters).map(|query| Some(self.expr(query))).collect();
                ("join", vec![Payload::Tables(sides)], children)
            }
        };
        node(Class::Relation, named(operation), vec![], vec![], payload, children, None, 0, 1)
    }
}

thread_local! {
    static CACHE: RefCell<HashMap<Expr, Rc<CanonicalKey>>> = RefCell::new(HashMap::new());
}

pub fn canonical_order_key(expr: &Expr, ctx: &dyn SortContext) -> Rc<CanonicalKey> {
    let mut keys = Keys { context: ctx, memo: HashMap::new(), in_reduce_body: ctx.in_reduce_body() };
    if expr.has_heap_ref() || ctx.temporal_aliases().is_some() || ctx.external_names().is_some()
        || ctx.in_reduce_body() || ctx.choice_slots().is_some() || ctx.reduce_shapes().is_some_and(|shapes| !shapes.is_empty())
    {
        return keys.expr(expr);
    }
    if let Some(key) = CACHE.with(|cache| cache.borrow().get(expr).cloned()) { return key; }
    let key = keys.expr(expr);
    crate::cache::with_entry_budget(expr.size(), || {
        CACHE.with(|cache| crate::cache::insert_bounded(&mut cache.borrow_mut(), expr.clone(), key.clone(),
            crate::cache::DEFAULT_KEY_LIMIT, "ordering_axiom"));
    });
    key
}

pub fn canonical_order_compare(left: &Expr, right: &Expr, ctx: &dyn SortContext) -> Ordering {
    canonical_order_key(left, ctx).cmp(&canonical_order_key(right, ctx))
}

/// CF-08: fix operands using the shared key and reverse the comparison token
/// only if the caller has already established this as a value comparison.
pub fn canonical_order_ordered_comparison(
    op: BinOpKind, left: &Expr, right: &Expr, ctx: &dyn SortContext,
) -> Option<(BinOpKind, bool)> {
    let opposite = op.swapped_comparison()?;
    canonical_order_compare(left, right, ctx).is_gt().then_some((opposite, true))
}

pub(crate) fn orient_ordered_expression(expr: &Expr, ctx: &dyn SortContext) -> Expr {
    if let ExprKind::BinOp { op, left, right } = expr.kind() {
        if let Some((op, true)) = canonical_order_ordered_comparison(*op, left, right, ctx) {
            return Expr::new(ExprKind::BinOp { op, left: right.clone(), right: left.clone() });
        }
    }
    expr.clone()
}

/// The value positions in which an outer LET can substitute without
/// entering a new branch, short-circuit guard, row, or acquisition scope.
pub fn let_eager_children(expr: &Expr) -> Vec<&Expr> {
    match expr.kind() {
        ExprKind::BinOp { op: BinOpKind::Add | BinOpKind::Mul | BinOpKind::Sub, left, right } => vec![left, right],
        ExprKind::UnaryOp { op: UnaryOpKind::Neg, operand } => vec![operand],
        ExprKind::ArrayLit(items) => items.iter().collect(),
        _ => vec![],
    }
}

fn value_key_with_lets(
    expression: &Expr, inline: &Rc<CanonicalKey>, bindings: &[Expr], uses: &mut [usize],
) -> Rc<CanonicalKey> {
    if let Some(slot) = bindings.iter().position(|definition| definition == expression) {
        uses[slot] += 1;
        return node(Class::Reference, named("let_reference"), inline.types.clone(), inline.contract.clone(),
            vec![], vec![], Some(Identity::Local(slot)), 0, 1);
    }
    let children = let_eager_children(expression);
    if bindings.is_empty() || children.is_empty() { return inline.clone(); }
    let mut key = inline.as_ref().clone();
    for (index, expr) in children.into_iter().enumerate() {
        let old = inline.children[index].as_ref().expect("eager key slot");
        let new = value_key_with_lets(expr, old, bindings, uses);
        key.arms -= &old.arms;
        key.arms += &new.arms;
        key.operations -= &old.operations;
        key.operations += &new.operations;
        key.children[index] = Some(new);
    }
    Rc::new(key)
}

/// A scoped RA-09 choice has two outer operands. The last child is
/// the YIELD value, whose references have already been assigned lexical slots.
pub type ChoiceOperands = (Rc<CanonicalKey>, Rc<CanonicalKey>);

pub fn named_parameter_key(name: &str, kind: &str) -> Rc<CanonicalKey> {
    binding(Class::Reference, kind, Identity::Parameter(path(name)))
}

fn wrap_choices(mut value: Rc<CanonicalKey>, prefix: &[ChoiceOperands]) -> Rc<CanonicalKey> {
    for (source, fallback) in prefix.iter().rev() {
        value = node(Class::Relation, named("choose"), value.types.clone(), value.contract.clone(),
            vec![], vec![Some(source.clone()), Some(fallback.clone()), Some(value)], None, 0, 1);
    }
    value
}

struct LetSearch<'a> {
    value: &'a Expr,
    inline: Rc<CanonicalKey>,
    inline_full: Rc<CanonicalKey>,
    choices: &'a [ChoiceOperands],
    candidates: Vec<(Expr, Rc<CanonicalKey>)>,
    best: (Rc<CanonicalKey>, Vec<Expr>),
    budget: usize,
}

impl LetSearch<'_> {
    fn search(&mut self, bindings: &mut Vec<Expr>, definition_keys: &mut Vec<Rc<CanonicalKey>>,
        definition_uses: &mut Vec<usize>, selected: &mut [bool]) -> Result<(), &'static str>
    {
        // Charge owned occurrences, not arena node count; a traversal may
        // visit each syntactically repeated child separately.
        let work = self.value.size().saturating_add(bindings.iter()
            .fold(1usize, |sum, expr| sum.saturating_add(expr.size())));
        self.budget = self.budget.checked_sub(work)
            .ok_or("LET candidate budget exhausted; no canonical result was produced")?;
        if !bindings.is_empty() {
            let mut uses = definition_uses.clone();
            let value = value_key_with_lets(self.value, &self.inline, bindings, &mut uses);
            let value = wrap_choices(value, self.choices);
            if uses.iter().all(|&count| count >= 2) {
                let declarations = definition_keys.iter().cloned().map(Some).collect();
                let mut key = node(Class::Construct, named("let"), self.inline.types.clone(),
                    self.inline.contract.clone(), vec![], declarations, None, 0, 0);
                let result = Rc::get_mut(&mut key).expect("new LET key");
                result.arms += &value.arms;
                result.operations += &value.operations;
                result.tail = Some(value);
                // The shared structure key only breaks ties between plans
                // that have already beaten the inline value in complexity.
                if (&key.arms, &key.operations) < (&self.inline_full.arms, &self.inline_full.operations)
                    && key < self.best.0
                {
                    self.best = (key, bindings.clone());
                }
            }
        }
        for index in 0..self.candidates.len() {
            if selected[index] { continue; }
            let (expression, inline) = &self.candidates[index];
            let mut uses = definition_uses.clone();
            let body = value_key_with_lets(expression, inline, bindings, &mut uses);
            if body.arms == BigUint::default() { continue; } // a LET reference does not own a CASE
            let binding = node(Class::Construct, named("let_binding"), body.types.clone(), body.contract.clone(),
                vec![], vec![Some(body)], None, 0, 1);
            selected[index] = true;
            uses.push(0);
            bindings.push(expression.clone());
            definition_keys.push(binding);
            self.search(bindings, definition_keys, &mut uses, selected)?;
            definition_keys.pop();
            bindings.pop();
            selected[index] = false;
        }
        Ok(())
    }
}

/// Compare complete CF-09 plans using this module's ordinary structural key.
/// Eligibility is supplied by the owner of the acquired value. Names are not
/// part of the search; reference slots are allocated by each candidate order.
pub fn canonical_let_plan(
    value: &Expr, ctx: &dyn SortContext, choices: &[ChoiceOperands], eligible: impl Fn(&Expr) -> bool,
) -> Result<Vec<Expr>, &'static str> {
    fn occurrences(expr: &Expr, eligible: &impl Fn(&Expr) -> bool, counts: &mut HashMap<Expr, usize>) {
        if expr.has_case_when() && eligible(expr) { *counts.entry(expr.clone()).or_default() += 1; }
        for child in let_eager_children(expr) { occurrences(child, eligible, counts); }
    }
    if !value.has_case_when() { return Ok(vec![]); }
    let mut counts = HashMap::new();
    occurrences(value, &eligible, &mut counts);
    let mut candidates: Vec<_> = counts.into_iter().filter(|(_, count)| *count >= 2)
        .map(|(expr, _)| { let key = canonical_order_key(&expr, ctx); (expr, key) }).collect();
    if candidates.is_empty() { return Ok(vec![]); }
    candidates.sort_by(|(_, a), (_, b)| a.cmp(b));
    let inline = canonical_order_key(value, ctx);
    let mut selected = vec![false; candidates.len()];
    let budget = std::env::var("NORMALIZER_MATH_MAX_CANDIDATE_NODES").ok()
        .and_then(|raw| raw.parse().ok()).unwrap_or(4_000_000);
    let inline_full = wrap_choices(inline.clone(), choices);
    let mut search = LetSearch { value, inline, inline_full: inline_full.clone(), choices,
        candidates, best: (inline_full, vec![]), budget };
    search.search(&mut Vec::new(), &mut Vec::new(), &mut Vec::new(), &mut selected)?;
    Ok(search.best.1)
}
