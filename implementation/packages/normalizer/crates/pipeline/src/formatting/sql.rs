//! formatting::sql: Expr → 类 SQL 字符串
//!
//! 规则：
//! - Lit(Int) / Lit(Str) / Lit(Bool) / Lit(Null) → "42" / "\"hello\"" / "true" / "null"
//! - SymVar(Parameter(name)) → name
//! - BinOp(Eq, a, b) → "a == b"
//! - BinOp(Add, a, b) → "a + b"
//! - UnaryOp(Not, e) → "NOT (e)" / UnaryOp(Neg, e) → "-(e)"
//! - CaseWhen(branches) → "CASE WHEN c1 THEN v1 WHEN c2 THEN v2 END"
//! - Call(key, args) → "funcName(arg1, arg2)"（用 math::func_name 反查）
//! - HeapRef(id) → verified business identity, or internal debug representation

mod relation_query;
pub(crate) mod scoped_relation;
pub(crate) mod business_value;

use crate::canonical_names::CanonicalNames;
use std::collections::HashMap;

use math::{BinOpKind, Expr, Lit, RelationAggregateKind, SymVarOrigin, UnaryOpKind};

/// Formatting context shared by one business result and its boundary events.
pub struct FormatCtx {
    projected_table: Option<String>,
    business_inputs: Option<std::collections::BTreeMap<String, String>>,
    captured_parameters: std::collections::BTreeMap<String, String>,
    nullable_columns: std::collections::BTreeSet<String>,
    scalar_arrays: std::collections::BTreeSet<String>,
    nested_arrays: std::collections::BTreeSet<String>,
    record_arrays: std::collections::BTreeSet<String>,
    temporal_names: std::collections::BTreeMap<u64, usize>,
    reductions: std::collections::BTreeMap<u64, math::ReduceShape>,
    total_temporals: std::collections::BTreeSet<u64>,
    relation_scope: scoped_relation::Scope,
    scoped_plans: Option<scoped_relation::ScopedPlans>,
    business_identities: std::collections::BTreeMap<i64, String>,
    heap_bindings: std::collections::BTreeMap<i64, math::HeapBindingIdentity>,
    business_projections: Vec<(Expr,String)>,
    business_states: std::collections::BTreeMap<u64,String>,
    observation_identities: std::collections::BTreeMap<u64, math::ObservationIdentity>,
    business_acquisitions: std::collections::BTreeMap<u64,String>,
    business_receipt_guards: std::collections::BTreeMap<u64, Expr>,
    business_normal_completions: std::collections::BTreeMap<u64, Expr>,
    business_observation_projections: Vec<(Expr, String)>,
    business_evaluation_guard: Option<Expr>,
    /// Public receipt ordinal; the raw binding ID remains an internal identity.
    e_renumber: HashMap<u64, usize>,

}

impl Default for FormatCtx {
    fn default() -> Self {
        FormatCtx {
            projected_table: None,
            business_inputs: None,
            captured_parameters: Default::default(),
            nullable_columns: Default::default(),
            scalar_arrays: Default::default(),
            nested_arrays: Default::default(),
            record_arrays: Default::default(),
            temporal_names: Default::default(),
            reductions: Default::default(),
            total_temporals: Default::default(),
            relation_scope: Default::default(),
            scoped_plans: None,
            business_identities: Default::default(),
            heap_bindings: Default::default(),
            business_projections: Vec::new(),
            business_states: Default::default(),
            observation_identities: Default::default(),
            business_acquisitions: Default::default(),
            business_receipt_guards: Default::default(),
            business_normal_completions: Default::default(),
            business_observation_projections: Vec::new(),
            business_evaluation_guard: None,
            e_renumber: HashMap::new(),
        }
    }
}

impl FormatCtx {
    fn projected_column_label<'a>(&self, path: &'a str) -> &'a str {
        if let (Some(table), Some((owner, column))) = (&self.projected_table, path.split_once('.')) {
            let conflicts = self.business_inputs.as_ref().is_some_and(|names|
                names.values().any(|name| name == column))
                || self.business_states.values().any(|name| name == column);
            if owner == table && !conflicts { return column; }
        }
        path
    }

    pub(crate) fn set_nested_arrays(&mut self, arrays: std::collections::BTreeSet<String>) {
        self.nested_arrays = arrays;
    }
    pub(crate) fn display_business_value(&mut self, value: Expr, label: String) {
        self.business_projections.push((value,label));
    }
    pub(crate) fn forget_business_value(&mut self) { self.business_projections.pop(); }
    pub(crate) fn display_business_observation(&mut self, value: Expr, label: String) {
        self.business_observation_projections.push((value, label));
    }
    pub(crate) fn bind_business_event(&mut self, id: u64, position: usize, identity: math::ObservationIdentity, guard: Expr) {
        self.e_renumber.insert(id, position);
        self.observation_identities.insert(id, identity);
        if guard.as_bool() != Some(true) {
            self.business_receipt_guards.insert(id, guard);
        }
    }

    pub(crate) fn bind_normal_completion(&mut self, id: u64, guard: Expr) {
        if guard.as_bool() != Some(true) { self.business_normal_completions.insert(id, guard); }
    }

    pub(crate) fn bind_acquisition(&mut self, id: u64, label: String, identity: math::ObservationIdentity, guard: Expr) {
        self.business_acquisitions.insert(id, label);
        self.observation_identities.insert(id, identity);
        if guard.as_bool() != Some(true) {
            self.business_receipt_guards.insert(id, guard);
        }
    }

    pub(crate) fn acquisition_name(&self, id: u64) -> Option<&str> {
        self.business_acquisitions.get(&id).map(String::as_str)
    }

    pub(crate) fn receipt_name(&self, id: u64) -> Option<String> {
        self.e_renumber.get(&id).map(|position| format!("$e_{position}"))
    }

    pub(crate) fn receipt_count(&self) -> usize {
        self.e_renumber.len()
    }

    pub(crate) fn observation_identities(&self) -> &std::collections::BTreeMap<u64, math::ObservationIdentity> {
        &self.observation_identities
    }

    pub(crate) fn set_captured_parameters(&mut self, parameters: std::collections::BTreeMap<String, String>) {
        self.captured_parameters = parameters;
    }
    pub(crate) fn captured_parameters(&self) -> Option<&std::collections::BTreeMap<String, String>> {
        (!self.captured_parameters.is_empty()).then_some(&self.captured_parameters)
    }

    pub(crate) fn bound_order(&self) -> CanonicalNames<'_> {
        let external = self.observation_identities();
        CanonicalNames {
            heap: &self.heap_bindings, captured: self.captured_parameters(), completion_classes: None,
            external: (!external.is_empty()).then_some(external),
            total_temporals: None, reductions: Some(&self.reductions),
            temporals: (!self.temporal_names.is_empty()).then_some(&self.temporal_names),
        }
    }

    pub(crate) fn set_temporal_names(&mut self, names: std::collections::BTreeMap<u64, usize>) {
        self.temporal_names = names;
    }

    pub(crate) fn set_reductions(
        &mut self, reductions: std::collections::BTreeMap<u64, math::ReduceShape>,
        total: std::collections::BTreeSet<u64>,
    ) {
        self.reductions = reductions;
        self.total_temporals = total;
    }
    fn is_total_reduce(&self, value: &Expr) -> bool {
        matches!(value.as_sym_var(), Some(SymVarOrigin::Temporal { binding_id, .. })
            if self.total_temporals.contains(binding_id) && self.reductions.contains_key(binding_id))
    }

    pub(crate) fn set_relation_slots(&mut self, slots: Vec<scoped_relation::Slot>) {
        self.relation_scope.slots = slots.into();
    }

    fn uncaptured_name(&self, namespace: &str, name: &str) -> String {
        let root = name.split('.').next().unwrap_or(name);
        if self.relation_scope.members.values().any(|member| member == root)
            || (self.relation_scope.in_reduce_yield && matches!(root, "acc" | "item"))
        {
            format!("{namespace}.{name}")
        } else {
            name.to_owned()
        }
    }

    fn relation_operand(&mut self, operand: &scoped_relation::Operand) -> String {
        use scoped_relation::Operand;
        match operand {
            Operand::Integer(value) => value.to_string(),
            Operand::Parameter(name) => self.uncaptured_name("P", name),
            Operand::Acquired(name) => self.uncaptured_name("E", name),
            Operand::Prior(slot) => self.relation_slot(*slot),
            Operand::Arithmetic(expression) => format_expr_with_ctx(expression, self),
        }
    }

    fn relation_slot(&mut self, slot: usize) -> String {
        use scoped_relation::Slot;
        let slots = self.relation_scope.slots.clone();
        match &slots[slot] {
            Slot::Fold { source, initial, body } => self.format_fold(source, initial, body, None),
            Slot::Choice { .. } => self.relation_scope.members.get(&slot)
                .expect("choice must be bound in its certified acquisition domain").clone(),
        }
    }

    fn format_fold(&mut self, source: &str, initial: &scoped_relation::Operand, body: &Expr, indent: Option<usize>) -> String {
        let source = self.uncaptured_name("P", source);
        let initial = self.relation_operand(initial);
        let prior = std::mem::replace(&mut self.relation_scope.in_reduce_yield, true);
        let body = format_expr_with_ctx(body, self);
        self.relation_scope.in_reduce_yield = prior;
        let gap = indent.map_or_else(|| " ".into(), |n| format!("\n{}", " ".repeat(n + "REDUCE(".len())));
        format!("REDUCE({source} FROM {initial} AS (acc, item){gap}YIELD {body})")
    }

    fn format_let_definition(&mut self, name: &str, expression: &Expr) -> String {
        let prefix = format!("LET {name} = ");
        if let Some(SymVarOrigin::Temporal { binding_id, .. }) = expression.as_sym_var() {
            if let Some(&slot) = self.temporal_names.get(binding_id) {
                let slots = self.relation_scope.slots.clone();
                if let scoped_relation::Slot::Fold { source, initial, body } = &slots[slot] {
                    return format!("{prefix}{}", self.format_fold(source, initial, body, Some(prefix.len())));
                }
            }
        }
        format!("{prefix}{}", format_expr_with_ctx(expression, self))
    }

    fn eligible_choices(
        &self, expression: &Expr, active: &Expr, bound: &[usize],
    ) -> Vec<usize> {
        let mut needed = std::collections::BTreeSet::new();
        let slots = self.relation_scope.slots.clone();
        if slots.is_empty() { return vec![]; }
        expression.walk(&mut |part| {
            if let Some(SymVarOrigin::Temporal { binding_id, .. }) = part.as_sym_var() {
                if let Some(&slot) = self.temporal_names.get(binding_id) {
                    scoped_relation::add_dependencies(&slots, slot, &mut needed);
                }
            }
        });
        needed.into_iter().filter(|slot| matches!(&slots[*slot], scoped_relation::Slot::Choice { guard, .. }
            if !bound.contains(slot) && math::builder::path_implies(active, guard))).collect()
    }

    fn choice_keys(&self, order: &[usize]) -> std::collections::BTreeMap<u64, usize> {
        self.temporal_names.iter().filter_map(|(&id, &slot)| {
            order.iter().position(|&active| active == slot).map(|position| (id, position))
        }).collect()
    }

    fn canonical_value_plan(
        &self, value: &Expr, external: &std::collections::BTreeMap<u64, math::ObservationIdentity>, active: &Expr, bound: &[usize],
    ) -> math::ValuePlan {
        let location = scoped_relation::ValueLocation {
            value: value.clone(), domain: active.clone(), members: bound.to_vec(),
            in_reduce_body: self.relation_scope.in_reduce_yield,
        };
        let cache = self.scoped_plans.as_ref().filter(|plans| &plans.external == external).map(|plans| &plans.plans);
        if let Some(plan) = cache.and_then(|cache| cache.borrow().get(&location).cloned()) { return plan; }
        let keys = self.prepare_value_keys(value, external, active, bound, true);
        let compare = crate::canonical_names::ScopedValueKeys {
            base: CanonicalNames { heap: &self.heap_bindings, captured: self.captured_parameters(), completion_classes: None,
                external: (!external.is_empty()).then_some(external), total_temporals: Some(&self.total_temporals),
                reductions: Some(&self.reductions), temporals: Some(&self.temporal_names) },
            slots: &keys.lexical, cases: &keys.cases, in_reduce_body: self.relation_scope.in_reduce_yield,
        };
        let payload = math::canonicalize_bound_integer_ops(value, &compare);
        let plan = math::canonical_let_plan(&payload, &compare, &keys.prefix,
            |expr| business_value::closed_candidate(expr, self))
            .unwrap_or_else(|failure| panic!("{failure}"));
        if let Some(cache) = cache { cache.borrow_mut().insert(location, plan.clone()); }
        plan
    }

    fn prepare_value_keys(
        &self, expression: &Expr, external: &std::collections::BTreeMap<u64, math::ObservationIdentity>,
        active: &Expr, bound: &[usize], enter: bool,
    ) -> scoped_relation::ValueKeys {
        use scoped_relation::{Slot, ValueKeys};
        let mut order = bound.to_vec();
        let mut prepared = ValueKeys { lexical: self.choice_keys(&order), ..ValueKeys::default() };
        let empty_cases = HashMap::new();
        let base = || CanonicalNames { heap: &self.heap_bindings, captured: self.captured_parameters(), completion_classes: None,
            external: (!external.is_empty()).then_some(external), total_temporals: None,
            reductions: Some(&self.reductions), temporals: Some(&self.temporal_names) };
        if enter {
            for slot in self.eligible_choices(expression, active, bound) {
                let Slot::Choice { shape, .. } = &self.relation_scope.slots[slot] else {
                    unreachable!("certified choice slot");
                };
                let compare = crate::canonical_names::ScopedValueKeys {
                    base: base(), slots: &prepared.lexical, cases: &empty_cases,
                    in_reduce_body: self.relation_scope.in_reduce_yield,
                };
                prepared.prefix.push((shape.source.key(&compare), shape.fallback.key(&compare)));
                order.push(slot);
                prepared.lexical = self.choice_keys(&order);
            }
        }
        if !expression.has_case_when() || self.business_inputs.is_none() { return prepared; }
        // Visit each CASE only from its enclosing scope. A structurally equal
        // CASE under a different arm has different availability and keys.
        expression.walk_pruned(&mut |part| {
            let Some(branches) = part.as_case_when() else { return part.has_case_when(); };
            if prepared.cases.contains_key(part) { return false; }
            let guard_key = |child: &Expr, domain: &Expr| {
                let inner = self.prepare_value_keys(child, external, domain, &order, false);
                let compare = crate::canonical_names::ScopedValueKeys {
                    base: base(), slots: &inner.lexical, cases: &inner.cases,
                    in_reduce_body: self.relation_scope.in_reduce_yield,
                };
                math::canonical_order_key(child, &compare)
            };
            let mut remaining = active.clone();
            let arms = branches.iter().map(|(guard, value)| {
                let (selected, next) = observation_arm_domains(&remaining, guard);
                // A guard uses the enclosing bindings. Starting a new
                // choice in the guard would not bind its selected arm.
                let pair = (guard_key(guard, &remaining),
                    self.canonical_value_plan(value, external, &selected, &order).key);
                remaining = next;
                pair
            }).collect();
            prepared.cases.insert(part.clone(), arms);
            false
        });
        prepared
    }

    pub(crate) fn in_relation_scope(&mut self, expression: &Expr, render: impl FnOnce(&mut Self) -> String) -> String {
        let active = self.relation_scope.active.clone().unwrap_or_else(|| math::builder::lit_bool(true));
        let needed = self.eligible_choices(expression, &active, &self.relation_scope.member_order);
        let slots = self.relation_scope.slots.clone();
        let mut bindings = Vec::new();
        for slot in needed {
            if let scoped_relation::Slot::Choice { members, empty, .. } = &slots[slot] {
                let empty = self.relation_operand(empty);
                let members = self.uncaptured_name("P", members);
                let name = format!("member{}", self.relation_scope.members.len());
                self.relation_scope.members.insert(slot, name.clone());
                self.relation_scope.member_order.push(slot);
                bindings.push((slot, format!("CHOOSE({members} DEFAULT {empty} AS {name} YIELD")));
            }
        }
        let mut block_yield = expression.as_array_lit().is_some();
        if !bindings.is_empty() {
            expression.walk_pruned(&mut |expr| {
                block_yield |= matches!(expr.as_sym_var(), Some(SymVarOrigin::Temporal { binding_id, .. })
                    if self.reductions.contains_key(binding_id));
                !block_yield
            });
        }
        let mut value = render(self);
        for (depth, (slot, prefix)) in bindings.into_iter().rev().enumerate() {
            self.relation_scope.members.remove(&slot);
            self.relation_scope.member_order.pop();
            value = if block_yield || depth > 0 {
                format!("{prefix}\n  {})", value.replace('\n', "\n  "))
            } else { format!("{prefix} {value})") };
        }
        value
    }

    pub(crate) fn set_business_identities(
        &mut self, roots: std::collections::BTreeMap<i64, String>,
        bindings: std::collections::BTreeMap<i64, math::HeapBindingIdentity>,
    ) {
        self.business_identities = roots;
        self.heap_bindings = bindings;
    }

    pub(crate) fn set_record_arrays(&mut self, arrays: std::collections::BTreeSet<String>) {
        self.record_arrays = arrays;
    }

    pub(crate) fn declare_nullable_columns(
        &mut self, tables: &std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>,
    ) {
        self.nullable_columns = tables.iter().flat_map(|(table, columns)| {
            columns.iter().filter(|(_, ty)| ty.ends_with('?'))
                .map(move |(column, _)| format!("{table}.{column}"))
        }).collect();
    }

    pub(crate) fn set_scalar_arrays(&mut self, arrays: std::collections::BTreeSet<String>) {
        self.scalar_arrays = arrays;
    }

    pub(crate) fn with_business_evaluation_guard<T>(
        &mut self,
        guard: &Expr,
        render: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let prior = self.business_evaluation_guard.replace(guard.clone());
        let rendered = render(self);
        self.business_evaluation_guard = prior;
        rendered
    }

    pub(crate) fn for_business(
        inputs: std::collections::BTreeMap<String, String>,
        business_states: std::collections::BTreeMap<u64, String>,
        state_roots: &std::collections::BTreeMap<u64, String>,
    ) -> Self {
        let observation_identities = state_roots.iter().map(|(&id, root)| {
            (id, math::ObservationIdentity::EntrySnapshot(vec![root.clone()]))
        }).collect();
        Self { business_inputs: Some(inputs), business_states, observation_identities, ..Self::default() }
    }
    pub fn new() -> Self {
        Self::default()
    }

    /// Resolve an external binding for display; closed business expressions are validated before rendering.
    pub(crate) fn e_slot(&self, raw: u64) -> Option<usize> {
        self.e_renumber.get(&raw).copied()
    }


}

/// Expr → 类 SQL 字符串（单入口，默认新建 ctx，无 heap）
pub fn format_expr_sql(expr: &Expr) -> String {
    let mut ctx = FormatCtx::new();
    format_expr_with_ctx(expr, &mut ctx)
}

/// Expr → 类 SQL 字符串（带 ctx，外部复用编号/heap）
pub fn format_expr_with_ctx(expr: &Expr, ctx: &mut FormatCtx) -> String {
    if let Some((_, label)) = ctx.business_projections.iter().rev().find(|(value, _)| value == expr) {
        return label.clone();
    }
    // Observation aliases remain visible in a selected arm. Unlike a
    // lexical LET alias they do not enlarge the value's acquisition domain.
    if let Some((_, label)) = ctx.business_observation_projections.iter().rev()
        .find(|(value, _)| value == expr
            && business_value::receipts_available(value, ctx.business_evaluation_guard.as_ref(), ctx))
    {
        return label.clone();
    }
    if ctx.business_projections.is_empty() || !math::let_eager_children(expr).is_empty() {
        return format_expr_inner(expr, ctx);
    }
    // The virtual LET key and rendering use the same occurrence scope.
    // A matching expression behind a guard or in a row is not a use here.
    let outer = std::mem::take(&mut ctx.business_projections);
    let rendered = format_expr_inner(expr, ctx);
    ctx.business_projections = outer;
    rendered
}

fn format_expr_inner(expr: &Expr, ctx: &mut FormatCtx) -> String {
    if ctx.business_inputs.is_some() {
        if let Some(entries) = math::domains::composite::ordered_text_map_entries(expr) {
            let rows = entries.iter().map(|entry| format_expr_with_ctx(entry, ctx)).collect::<Vec<_>>();
            return format!("MAP([{}])", rows.join(", "));
        }
        if let Some(vector) = math::domains::composite::vector_length_input(expr) {
            return format!("LENGTH({})", format_expr_with_ctx(vector, ctx));
        }
        if let Some((record, name, _)) = math::domains::composite::field_projection(expr) {
            return format!("{}.{name}", format_expr_with_ctx(record, ctx));
        }
        if let Some(operation) = math::domains::decimal::view(expr) {
            use math::domains::decimal::View;
            return match operation {
                View::Literal(value, scale) => math::domains::decimal::display_literal(value, scale),
                View::Multiply(a, b) => {
                    let format_operand = |operand: &Expr, ctx: &mut FormatCtx| {
                        let value = format_expr_with_ctx(operand, ctx);
                        if operand.as_case_when().is_some() { format!("({value})") } else { value }
                    };
                    format!("{} * {}", format_operand(a, ctx), format_operand(b, ctx))
                }
                View::Round(value, scale, mode) =>
                    format!("N.round({}, {}, N.{})", format_expr_with_ctx(value, ctx),
                        format_expr_with_ctx(scale, ctx), mode.name()),
            };
        }
    }
    // 优先查 Lit
    if let Some(lit) = expr.as_lit() {
        return format_lit(lit);
    }
    // ArrayLit is an immutable value expression, independent of heap identity.
    if let Some(items) = expr.as_array_lit() {
        return format!(
            "[{}]",
            items
                .iter()
                .map(|item| {
                    let rendered = format_expr_with_ctx(item, ctx);
                    if ctx.business_inputs.is_some() && item.as_case_when().is_some() {
                        format!("({rendered})")
                    } else {
                        rendered
                    }
                })
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    if let Some(origin) = expr.as_sym_var() {
        return format_sym_var(origin, ctx);
    }
    if let Some(alias) = expr.as_row_value() {
        return ctx.projected_column_label(alias).to_owned();
    }
    if let Some(id) = expr.as_heap_ref() {
        // Business projection verifies named identities before rendering.
        return ctx.business_identities.get(&id).cloned().unwrap_or_else(|| format!("{expr:?}"));
    }
    if let Some((op, operand)) = expr.as_unary_op() {
        match op {
            UnaryOpKind::Not => {
                if let Some(rendered) = try_format_not_exists_as_count_zero(operand, ctx)
                    .or_else(|| relation_query::render_empty_test(operand, ctx))
                {
                    return rendered;
                }
                let inner = format_expr_with_ctx(operand, ctx);
                return format!("NOT ({})", inner);
            }
            UnaryOpKind::Neg => {
                let inner = format_expr_with_ctx(operand, ctx);
                return format!("- ({})", inner);
            }
        }
    }
    if let Some((op, left, right)) = expr.as_binop() {
        if let Some(rendered) = business_value::format_guarded_boolean(op, left, right, ctx) {
            return rendered;
        }
        let domain = ctx.relation_scope.active.clone().unwrap_or_else(|| math::builder::lit_bool(true));
        let external = ctx.scoped_plans.as_ref().map_or(ctx.observation_identities(), |plans| &plans.external);
        let let_arithmetic = !ctx.business_projections.is_empty() && matches!(op, BinOpKind::Add | BinOpKind::Mul);
        let keys = if op.swapped_comparison().is_some() || let_arithmetic {
            ctx.prepare_value_keys(expr, external, &domain, &ctx.relation_scope.member_order, false)
        } else {
            scoped_relation::ValueKeys::default()
        };
        let public = crate::canonical_names::ScopedValueKeys { base: CanonicalNames {
            heap: &ctx.heap_bindings, captured: ctx.captured_parameters(), completion_classes: None,
            external: (!external.is_empty()).then_some(external),
            total_temporals: None, reductions: Some(&ctx.reductions),
            temporals: (!ctx.temporal_names.is_empty()).then_some(&ctx.temporal_names),
        }, slots: &keys.lexical, cases: &keys.cases, in_reduce_body: ctx.relation_scope.in_reduce_yield };
        // Only an actual reversal changes the original operands. Both
        // orientations use the established binary rendering below.
        let let_bindings = if let_arithmetic {
            ctx.business_projections.iter().map(|(expr, _)| expr.clone()).collect()
        } else { Vec::new() };
        let (op, left, right) = match math::canonical_order_ordered_comparison(op, left, right, &public) {
            Some((reversed, true)) => (reversed, right, left),
            _ if math::let_arithmetic_needs_swap(expr, &let_bindings, &public) => (op, right, left),
            _ => (op, left, right),
        };
        let l = format_binop_child_with_precedence(op, left, false, ctx);
        let r = format_binop_child_with_precedence(op, right, true, ctx);
        let token = match (op, ctx.business_inputs.is_some()) {
            (BinOpKind::And, true) => "AND",
            (BinOpKind::Or, true) => "OR",
            _ => binop_display_token(op),
        };
        return format!("{l} {token} {r}");
    }
    if let Some(branches) = expr.as_case_when() {
        return format_case_when(branches, ctx);
    }
    // All other shapes have already been handled. The remaining non-call
    // shape must be rendered by the relation owner or rejected there.
    let Some((key, args)) = expr.as_call() else {
        return format_scalar_array_observation(expr, ctx)
            .or_else(|| relation_query::render(expr, ctx))
            .expect("[NORM-BUSINESS-EXPRESSION] relation has no canonical PERS renderer");
    };
    let registered_name = math::func_name(key);
    if math::domains::integer_quotient::is_quotient(expr) {
        return format!("{}({}, {})", math::domains::integer_quotient::OUTPUT_NAME,
            format_expr_with_ctx(&args[0], ctx), format_expr_with_ctx(&args[1], ctx));
    }
    if ctx.business_inputs.is_some() {
        if let [value] = args {
            if registered_name == math::domains::source_type_guard::TYPEOF {
                return format!("typeof({})", format_expr_with_ctx(value, ctx));
            }
            if registered_name == math::domains::source_type_guard::TEXT_PROJECTION {
                return format_expr_with_ctx(value, ctx);
            }
        }
        if let Some((root, position, field)) = math::domains::relation::positional::field_projection(expr) {
            let position = format_expr_with_ctx(position, ctx);
            return format!("{}[{position}].{field}", array_name(root, ctx));
        }
        if let Some((operation, response, class)) = math::domains::completion::class_observation(&registered_name, args) {
            let response = format_expr_with_ctx(response, ctx);
            return if operation == "completion.is_return" {
                format!("{operation}({response})")
            } else { format!("{operation}({response}, {class})") };
        }
        match (registered_name.as_str(), args) {
            (crate::normalized_text::TRIM_KEY, [value]) => {
                return format!("N.trim({})", format_expr_with_ctx(value, ctx));
            }
            (crate::normalized_text::DEFAULT_IF_BLANK_KEY, [value, fallback]) => {
                return format!("N.defaultIfBlank({}, {})",
                    format_expr_with_ctx(value, ctx), format_expr_with_ctx(fallback, ctx));
            }
            _ => {}
        }
    }
    let name = math::domains::completion::output_name(math::domains::composite::output_name(
        &registered_name,
    ));
    if name == "value.input" {
        if let Some(input) = math::domains::composite::input_name(expr) {
            return input.to_owned();
        }
    }
    let args_str: Vec<String> = args.iter().map(|a| format_expr_with_ctx(a, ctx)).collect();
    format!("{}({})", name, args_str.join(", "))
}

// Count/Exists use the same exact bare-array observation in both
// single guards and the folded union of failure guards.
fn array_cardinality_name(expr: &Expr, expected: RelationAggregateKind) -> Option<&str> {
    use math::domains::relation::RelationExpr::Base;
    use RelationAggregateKind::{Count, Exists};
    match (expected, expr.as_relation_aggregate()?) {
        (Count, (Count, Base { name, .. }, None, None)) => Some(name),
        (Exists, (Exists, Base { name, .. }, _, None)) => Some(name),
        _ => None,
    }
}

fn try_format_not_exists_as_count_zero(expr: &Expr, ctx: &FormatCtx) -> Option<String> {
    let name = array_cardinality_name(expr, RelationAggregateKind::Exists)?;
    (ctx.scalar_arrays.contains(name) || ctx.record_arrays.contains(name))
        .then(|| format!("0 == LENGTH({})", array_name(name, ctx)))
}

fn array_name(raw: &str, ctx: &FormatCtx) -> String {
    let name = ctx.business_inputs.as_ref().and_then(|names| names.get(raw)).map_or(raw, String::as_str);
    ctx.uncaptured_name("P", name)
}

/// Failure on either of two fixed indices of the same non-null sequence
/// is a single upper bound. This does not compare separate array instances.
pub(crate) fn format_array_failure_union(expr: &Expr, ctx: &FormatCtx) -> Option<String> {
    fn extract<'a>(expr: &'a Expr) -> Option<(&'a str, i64)> {
        if let Some((BinOpKind::Or, a, b)) = expr.as_binop() {
            let (left, lower) = extract(a)?;
            let (right, upper) = extract(b)?;
            return (left == right).then_some((left, lower.max(upper)));
        }
        if let Some((UnaryOpKind::Not, exists)) = expr.as_unary_op() {
            return Some((array_cardinality_name(exists, RelationAggregateKind::Exists)?, 0));
        }
        let (op @ (BinOpKind::Le | BinOpKind::Eq), len, index) = expr.as_binop()? else { return None };
        let name = array_cardinality_name(len, RelationAggregateKind::Count)?;
        let Some(Lit::Int(i)) = index.as_lit() else { return None };
        (*i >= 0 && (op == BinOpKind::Le || *i == 0)).then_some((name, *i))
    }
    if !matches!(expr.as_binop(), Some((BinOpKind::Or, ..))) { return None; }
    let (name, upper) = extract(expr)?;
    if !ctx.scalar_arrays.contains(name) { return None; }
    Some(if upper == 0 {
        format!("0 == LENGTH({})", array_name(name, ctx))
    } else {
        format!("{} > LENGTH({})", math::Integer::from(upper) + 1, array_name(name, ctx))
    })
}

fn format_scalar_array_observation(expr: &Expr, ctx: &mut FormatCtx) -> Option<String> {
    use math::domains::relation::RelationExpr::Base;
    use RelationAggregateKind::{Count, Exists, NthValue};
    let (kind, Base { name, .. }, select, default) = expr.as_relation_aggregate()? else {
        return None;
    };
    if !ctx.scalar_arrays.contains(name) {
        return (ctx.record_arrays.contains(name) && matches!((kind, select, default), (Count, None, None)))
            .then(|| format!("LENGTH({})", array_name(name, ctx)));
    }
    let array = array_name(name, ctx).to_owned();
    match (kind, select, default) {
        (Count, None, None) => Some(format!("LENGTH({array})")),
        (Exists, _, None) => Some(format!("0 < LENGTH({array})")),
        (NthValue, Some(index), _) =>
            Some(format!("{array}[{}]", format_expr_with_ctx(index, ctx))),
        _ => None,
    }
}

fn binop_display_token(op: BinOpKind) -> &'static str {
    match op {
        BinOpKind::Add => "+",
        BinOpKind::Sub => "-",
        BinOpKind::Mul => "*",
        BinOpKind::Eq => "==",
        BinOpKind::Ne => "!=",
        BinOpKind::Lt => "<",
        BinOpKind::Le => "<=",
        BinOpKind::Gt => ">",
        BinOpKind::Ge => ">=",
        BinOpKind::And => "&&",
        BinOpKind::Or => "||",
    }
}

fn format_binop_child_with_precedence(
    parent_op: BinOpKind,
    child: &Expr,
    right: bool,
    ctx: &mut FormatCtx,
) -> String {
    let rendered = format_expr_with_ctx(child, ctx);
    let Some((child_op, _, _)) = child.as_binop() else {
        return rendered;
    };

    let precedence = |op| match op {
        BinOpKind::Or => 0,
        BinOpKind::And => 1,
        BinOpKind::Eq | BinOpKind::Ne => 2,
        BinOpKind::Lt | BinOpKind::Le | BinOpKind::Gt | BinOpKind::Ge => 3,
        BinOpKind::Add | BinOpKind::Sub => 4,
        BinOpKind::Mul => 5,
    };
    let parent = precedence(parent_op);
    let child = precedence(child_op);
    let needs_parens = child < parent || (child == parent && (right || parent == 2 || parent == 3));

    if needs_parens {
        format!("({})", rendered)
    } else {
        rendered
    }
}

// The FALSE continuation of a host guard is useful proof, but SQL
// UNKNOWN also skips WHEN. In that case the next arm gains no inverse fact.
fn observation_arm_domains(remaining: &Expr, guard: &Expr) -> (Expr, Expr) {
    use math::builder as b;
    if guard.may_observe_sql_unknown() {
        (math::normalize(b::and(remaining.clone(), guard.clone())), remaining.clone())
    } else { b::first_match_domains(remaining.clone(), guard) }
}

fn format_case_when(branches: &[(Expr, Expr)], ctx: &mut FormatCtx) -> String {
    let business = ctx.business_inputs.is_some();
    let mut parts = vec!["CASE".to_string()];
    let scoped = !ctx.relation_scope.slots.is_empty();
    let outer_domain = ctx.relation_scope.active.clone();
    let mut remaining = match (&outer_domain, &ctx.business_evaluation_guard) {
        (Some(a), Some(z)) if a != z => math::normalize(math::builder::and(a.clone(), z.clone())),
        (Some(domain), _) | (_, Some(domain)) => domain.clone(),
        _ => math::builder::lit_bool(true),
    };
    for (guard, value) in branches {
        let (text, test) = if business {
            let (selected, next) = observation_arm_domains(&remaining, guard);
            ctx.relation_scope.active = Some(selected.clone());
            let text = ctx.with_business_evaluation_guard(&selected, |ctx|
                business_value::format_value(value, ctx));
            ctx.relation_scope.active = Some(remaining.clone());
            let test = (guard.as_bool() != Some(true)).then(||
                ctx.with_business_evaluation_guard(&remaining, |ctx| format_expr_with_ctx(guard, ctx)));
            remaining = next;
            (text, test)
        } else {
            (format_expr_with_ctx(value, ctx),
                (guard.as_bool() != Some(true)).then(|| format_expr_with_ctx(guard, ctx)))
        };
        if let Some(test) = test { parts.push(format!("WHEN {test} THEN {text}")); }
        else {
            parts.push(format!("ELSE {text}"));
            break;
        }
    }
    ctx.relation_scope.active = outer_domain;
    let array_index_arm = business && branches.len() == 2
        && branches.iter().any(|(_, value)| value.as_relation_aggregate().is_some_and(|(kind, relation, _, _)| {
            kind == RelationAggregateKind::NthValue
                && matches!(relation, math::domains::relation::RelationExpr::Base { name, .. }
                    if ctx.scalar_arrays.contains(name))
        }));
    if business && branches.len() == 2 && !parts.iter().any(|part| part.contains("\nLET") || part.contains("THEN LET") || part.contains("ELSE LET"))
        && (scoped || branches[0].0.as_sym_var().is_some() || array_index_arm) {
        parts.push("END".into());
        return parts.join(" ");
    }
    let parts = if business {
        parts.into_iter().map(|part| {
            if part.contains("THEN LET ") || part.starts_with("ELSE LET ") {
                // A LET/IN block owns its own alignment and lexical scope.
                part
            } else { part.replace('\n', "\n  ") }
        }).collect::<Vec<_>>()
    } else { parts };
    format!("{}\nEND", parts.join("\n  "))
}

fn format_lit(lit: &Lit) -> String {
    match lit {
        Lit::Int(n) => n.to_string(),
        Lit::BigInt(n) => n.to_string(),
        Lit::Str(s) => serde_json::to_string(s).expect("string literal serialization"),
        Lit::StrUnits(units) => {
            use std::fmt::Write;
            let mut quoted = String::from('"');
            for unit in units {
                write!(quoted, "\\u{unit:04X}").expect("write into string");
            }
            quoted.push('"');
            quoted
        },
        Lit::Bool(b) => b.to_string(),
        Lit::Null => "null".to_string(),
    }
}

fn format_sym_var(origin: &SymVarOrigin, ctx: &mut FormatCtx) -> String {
    match origin {
        SymVarOrigin::Parameter { name, .. } => {
            if ctx.relation_scope.in_reduce_yield {
                if let Some((_, display)) = math::ReduceBodyContext::resolve(name) {
                    return display.to_owned();
                }
            }
            let display = ctx.business_inputs.as_ref().and_then(|names| names.get(name)).map_or(name.as_str(), String::as_str);
            ctx.uncaptured_name("P", display)
        }
        SymVarOrigin::External {
            binding_id, path, ..
        } => {
            if let Some(root) = ctx.business_states.get(binding_id).or_else(|| ctx.business_acquisitions.get(binding_id)) {
                let root = ctx.uncaptured_name("E", root);
                return std::iter::once(root.as_str()).chain(path.iter().map(String::as_str)).collect::<Vec<_>>().join(".");
            }
            let n = if ctx.business_inputs.is_some() {
                ctx.e_slot(*binding_id).expect("PERS invariant: unbound receipt or acquisition") as u64
            } else {
                *binding_id
            };
            if path.is_empty() {
                format!("$e_{}", n)
            } else {
                format!("$e_{}.{}", n, path.join("."))
            }
        }
        SymVarOrigin::Temporal { binding_id, primitive_type } => {
            let slot = if matches!(primitive_type.as_str(), "integer" | math::builder::RECORD_TEMPORAL_TAG) && ctx.business_inputs.is_some() {
                ctx.temporal_names.get(binding_id).copied().expect("closed temporal definition")
            } else { *binding_id as usize };
            if ctx.business_inputs.is_some() && ctx.temporal_names.contains_key(binding_id) {
                ctx.relation_slot(slot)
            } else {
                format!("$t_{slot}")
            }
        }
    }
}
