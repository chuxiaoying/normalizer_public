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
pub(crate) mod business_value;

use crate::canonical_names::CanonicalNames;
use std::collections::HashMap;

use math::{BinOpKind, Expr, Lit, RelationAggregateKind, SymVarOrigin, UnaryOpKind};

/// Formatting context shared by one business result and its boundary events.
pub struct FormatCtx {
    business_inputs: Option<std::collections::BTreeMap<String, String>>,
    nullable_columns: std::collections::BTreeSet<String>,
    scalar_arrays: std::collections::BTreeSet<String>,
    nested_arrays: std::collections::BTreeSet<String>,
    record_arrays: std::collections::BTreeSet<String>,
    temporal_names: std::collections::BTreeMap<u64, usize>,
    business_identities: std::collections::BTreeMap<i64, String>,
    business_projections: Vec<(Expr,String)>,
    business_states: std::collections::BTreeMap<u64,String>,
    business_receipt_guards: std::collections::BTreeMap<u64, Expr>,
    business_evaluation_guard: Option<Expr>,
    /// PERST: External binding_id → 0-based 编号
    e_renumber: HashMap<u64, usize>,

}

impl Default for FormatCtx {
    fn default() -> Self {
        FormatCtx {
            business_inputs: None,
            nullable_columns: Default::default(),
            scalar_arrays: Default::default(),
            nested_arrays: Default::default(),
            record_arrays: Default::default(),
            temporal_names: Default::default(),
            business_identities: Default::default(),
            business_projections: Vec::new(),
            business_states: Default::default(),
            business_receipt_guards: Default::default(),
            business_evaluation_guard: None,
            e_renumber: HashMap::new(),
        }
    }
}

impl FormatCtx {
    pub(crate) fn set_nested_arrays(&mut self, arrays: std::collections::BTreeSet<String>) {
        self.nested_arrays = arrays;
    }
    pub(crate) fn display_business_value(&mut self, value: Expr, label: String) {
        self.business_projections.push((value,label));
    }
    pub(crate) fn forget_business_value(&mut self) { self.business_projections.pop(); }
    pub(crate) fn bind_business_event(&mut self, id: u64, position: usize, guard: Expr) {
        self.e_renumber.insert(id, position);
        if guard.as_bool() != Some(true) {
            self.business_receipt_guards.insert(id, guard);
        }
    }

    pub(crate) fn published_external_names(&self) -> std::collections::BTreeMap<u64, String> {
        let mut names = self.business_states.clone();
        names.extend(self.e_renumber.iter().map(|(&id, position)| (id, format!("$e_{position}"))));
        names
    }

    pub(crate) fn set_temporal_names(&mut self, names: std::collections::BTreeMap<u64, usize>) {
        self.temporal_names = names;
    }

    pub(crate) fn set_business_identities(&mut self, roots: std::collections::BTreeMap<i64, String>) {
        self.business_identities = roots;
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

    pub(crate) fn for_business(inputs: std::collections::BTreeMap<String, String>, business_states: std::collections::BTreeMap<u64,String>) -> Self {
        Self { business_inputs: Some(inputs), business_states, ..Self::default() }
    }
    pub fn new() -> Self {
        Self::default()
    }

    /// 查 External 的 renumber 结果；PERST 主格式中若 bindings 存在但未命中，说明 R 引用了未登记的 $e。
    pub(crate) fn e_slot(&self, raw: u64) -> usize {
        self.e_renumber.get(&raw).copied().unwrap_or(raw as usize)
    }


}

/// Expr → 类 SQL 字符串（单入口，默认新建 ctx，无 heap）
pub fn format_expr_sql(expr: &Expr) -> String {
    let mut ctx = FormatCtx::new();
    format_expr_with_ctx(expr, &mut ctx)
}

/// Expr → 类 SQL 字符串（带 ctx，外部复用编号/heap）
pub fn format_expr_with_ctx(expr: &Expr, ctx: &mut FormatCtx) -> String {
    if ctx.business_inputs.is_some() {
        if let Some(entries) = math::domains::composite::ordered_text_map_entries(expr) {
            let rows = entries.iter().map(|entry| format_expr_with_ctx(entry, ctx)).collect::<Vec<_>>();
            return format!("MAP([{}])", rows.join(", "));
        }
        if let Some(vector) = math::domains::composite::vector_length_input(expr) {
            return format!("LENGTH({})", format_expr_with_ctx(vector, ctx));
        }
        if let Some((_, label)) = ctx.business_projections.iter().rev().find(|(value, _)| value == expr) {
            return label.clone();
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
                    format!(r#"ROUND({}, {scale}, "{}")"#, format_expr_with_ctx(value, ctx), mode.name()),
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
        return alias.to_owned();
    }
    if let Some(id) = expr.as_heap_ref() {
        // Business projection verifies named identities before rendering.
        return ctx.business_identities.get(&id).cloned().unwrap_or_else(|| format!("{expr:?}"));
    }
    if let Some((op, operand)) = expr.as_unary_op() {
        match op {
            UnaryOpKind::Not => {
                if let Some(rendered) = try_format_not_exists_as_count_zero(operand, ctx) {
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
        if let Some(rendered) = try_format_array_bound(op, left, right, ctx) {
            return rendered;
        }
        if let Some(rendered) = business_value::format_guarded_boolean(op, left, right, ctx) {
            return rendered;
        }
        let public = CanonicalNames {
            heap: &ctx.business_identities,
            external: (!ctx.business_states.is_empty()).then_some(&ctx.business_states),
            total_temporals: None, temporals: (!ctx.temporal_names.is_empty()).then_some(&ctx.temporal_names),
        };
        // Only an actual reversal changes the original operands. Both
        // orientations use the established binary rendering below.
        let (op, left, right) = match math::canonical_order_ordered_comparison(op, left, right, &public) {
            Some((reversed, true)) => (reversed, right, left),
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
        if ctx.business_inputs.is_some() {
            if let Some(selection) = business_value::format_selection(expr, ctx) {
                return selection;
            }
        }
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
    if ctx.business_inputs.is_some() {
        if let Some((root, position, field)) = math::domains::relation::positional::field_projection(expr) {
            let position = format_expr_with_ctx(position, ctx);
            return format!("{}[{position}].{field}", array_name(root, ctx));
        }
        if let Some((operation, response, class)) = math::domains::completion::class_observation(&registered_name, args) {
            return format!("{operation}({}, {class})", format_expr_with_ctx(response, ctx));
        }
        let display = match registered_name.as_str() {
            crate::normalized_text::TRIM_KEY => Some("TRIM"),
            crate::normalized_text::KEY => Some("blank"),
            _ => None,
        };
        if let (Some(display), [value]) = (display, args) {
            return format!("{display}({})", format_expr_with_ctx(value, ctx));
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
        .then(|| format!("LENGTH({}) == 0", array_name(name, ctx)))
}

fn array_name<'a>(raw: &'a str, ctx: &'a FormatCtx) -> &'a str {
    ctx.business_inputs.as_ref().and_then(|names| names.get(raw)).map_or(raw, String::as_str)
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
    Some(format!("LENGTH({}) <= {upper}", array_name(name, ctx)))
}

fn try_format_array_bound(op: BinOpKind, left: &Expr, right: &Expr, ctx: &FormatCtx) -> Option<String> {
    if op != BinOpKind::Gt { return None; }
    let name = array_cardinality_name(left, RelationAggregateKind::Count)?;
    let Some(Lit::Int(index)) = right.as_lit() else { return None };
    if !ctx.scalar_arrays.contains(name) || *index <= 0 { return None; }
    Some(format!("LENGTH({}) >= {}", array_name(name, ctx), index.checked_add(1)?))
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
        (Exists, _, None) => Some(format!("LENGTH({array}) > 0")),
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

    let needs_parens = matches!(
        (parent_op, child_op),
        (BinOpKind::And, BinOpKind::Or)
            | (BinOpKind::Mul, BinOpKind::Add)
            | (BinOpKind::Mul, BinOpKind::Sub)
    ) || (right && matches!((parent_op,child_op),
        (BinOpKind::Sub, BinOpKind::Add | BinOpKind::Sub)
        | (BinOpKind::Add, BinOpKind::Sub)));


    if needs_parens {
        format!("({})", rendered)
    } else {
        rendered
    }
}

fn format_case_when(branches: &[(Expr, Expr)], ctx: &mut FormatCtx) -> String {
    let business = ctx.business_inputs.is_some();
    let mut parts = vec!["CASE".to_string()];
    for (guard, value) in branches {
        let value = format_expr_with_ctx(value, ctx);
        if guard.as_bool() == Some(true) {
            parts.push(format!("ELSE {value}"));
            break;
        }
        let guard = format_expr_with_ctx(guard, ctx);
        parts.push(format!("WHEN {guard} THEN {value}"));
    }
    let array_index_arm = business && branches.len() == 2
        && branches[1].1.as_relation_aggregate().is_some_and(|(kind, relation, _, _)| {
            kind == RelationAggregateKind::NthValue
                && matches!(relation, math::domains::relation::RelationExpr::Base { name, .. }
                    if ctx.scalar_arrays.contains(name))
        });
    if business && branches.len() == 2 && (branches[0].0.as_sym_var().is_some() || array_index_arm) {
        parts.push("END".into());
        return parts.join(" ");
    }
    format!("{}\nEND", parts.join("\n  "))
}

fn format_lit(lit: &Lit) -> String {
    match lit {
        Lit::Int(n) => n.to_string(),
        Lit::BigInt(n) => n.to_string(),
        Lit::Str(s) => serde_json::to_string(s).expect("string literal serialization"),
        Lit::Bool(b) => b.to_string(),
        Lit::Null => "null".to_string(),
    }
}

fn format_sym_var(origin: &SymVarOrigin, ctx: &FormatCtx) -> String {
    match origin {
        SymVarOrigin::Parameter { name, .. } => {
            if let Some(display) = ctx.business_inputs.as_ref().and_then(|names| names.get(name)) { return display.clone(); }
            name.clone()
        }
        SymVarOrigin::External {
            binding_id, path, ..
        } => {
            if let Some(root) = ctx.business_states.get(binding_id) {
                return std::iter::once(root.as_str()).chain(path.iter().map(String::as_str)).collect::<Vec<_>>().join(".");
            }
            let n = ctx.e_slot(*binding_id);
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
            format!("$t_{slot}")
        }
    }
}
