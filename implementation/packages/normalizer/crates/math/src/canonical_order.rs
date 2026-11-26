//! 统一正则规范 (Unified Canonical Form): shared structural ordering.
//!
//! This module is the shared `expr.compareTo(ctx)` foundation.
//! Callers may use it only after they have proven the surrounding expression
//! family is commutative/associative (for example AND/OR).  It is not a license
//! to reorder CaseWhen arms, side-effect ledgers, arguments, list elements, or
//! string concatenation.
//!
//! The design contract is `design/unified-canonical-form.md`. The shared key is:
//!
//! ```text
//! subject_key
//!   -> predicate_family_rank
//!   -> polarity_rank
//!   -> operator_rank
//!   -> rhs_key_or_payload_key
//!   -> structural_tie_breaker
//! ```
//!
//! The implementation is ExprIR-oriented, not final rendered-string oriented:
//! public HeapRef / binding names are obtained through [`SortContext`] before
//! ordering, and raw ids are only last-resort stability fallback.

use crate::domains::primitive::registry::func_name;
use crate::expr::{BinOpKind, Expr, ExprKind, Lit, RelationAggregateKind, UnaryOpKind};
use crate::sort_context::SortContext;
use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::HashMap;

thread_local! {
    static CANONICAL_ORDER_CACHE: RefCell<HashMap<Expr, String>> = RefCell::new(HashMap::new());
}
/// Unified display comparison for peer expressions.
///
/// This is the single micro-normalization ordering kernel: callers that have
/// already proven a position is reorderable must compare expressions here,
/// rather than inventing local rank/string rules.
pub fn canonical_order_compare(left: &Expr, right: &Expr, ctx: &dyn SortContext) -> Ordering {
    canonical_order_key(left, ctx).cmp(&canonical_order_key(right, ctx))
}

/// Display-side orientation for comparison predicates.
///
/// The expression semantics and boolean normal form are decided before display.
/// This function only chooses which equivalent comparison spelling is printed,
/// using the same display-order kernel and fixed operator reversal.
pub fn canonical_order_ordered_comparison(
    op: BinOpKind,
    left: &Expr,
    right: &Expr,
    ctx: &dyn SortContext,
) -> Option<(BinOpKind, bool)> {
    match op {
        BinOpKind::Eq | BinOpKind::Ne => {
            if canonical_order_orient_equality_to_rhs(left, right, ctx) {
                Some((op, true))
            } else {
                None
            }
        }
        BinOpKind::Lt | BinOpKind::Le | BinOpKind::Gt | BinOpKind::Ge => {
            if canonical_order_orient_ordered_predicate_to_rhs(op, left, right, ctx) {
                Some((op.swapped_comparison()?, true))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Deterministic display-order key for peer terms in a proven commutative family.
pub fn canonical_order_key(expr: &Expr, ctx: &dyn SortContext) -> String {
    // Expr is immutable. Without scoped heap, environment or temporal names this key is
    // independent of SortContext, so the bounded cache is semantics-preserving and
    // avoids recomputing the same DisplayIR key in high-frequency AND/OR sorts.
    if !expr.has_heap_ref() && ctx.temporal_aliases().is_none() && ctx.external_names().is_none() {
        if let Some(cached) = CANONICAL_ORDER_CACHE.with(|cache| cache.borrow().get(expr).cloned()) {
            if crate::profile::cache_observation_enabled() {
                crate::profile::cache_lookup(
                    "ordering",
                    "ordering_axiom",
                    true,
                    expr.structural_hash(),
                    expr.size(),
                    "ordering_axiom_key",
                );
            }
            return cached;
        }
        if crate::profile::cache_observation_enabled() {
            crate::profile::cache_lookup(
                "ordering",
                "ordering_axiom",
                false,
                expr.structural_hash(),
                expr.size(),
                "ordering_axiom_key",
            );
        }
        let out = canonical_order_key_uncached(expr, ctx);
        let cache_weight = expr.size();
        if cache_weight <= crate::profile::cache_entry_limit() {
            CANONICAL_ORDER_CACHE.with(|cache| {
                let mut cache = cache.borrow_mut();
                // Preserve the ordering table's historical one-key offset.
                crate::cache::insert_bounded(&mut cache, expr.clone(), out.clone(),
                    crate::cache::DEFAULT_KEY_LIMIT + 1, "ordering_axiom");
            });
        } else {
            crate::profile::cache_admission_skip("ordering_axiom", cache_weight);
        }
        return out;
    }

    canonical_order_key_uncached(expr, ctx)
}

fn canonical_order_key_uncached(expr: &Expr, ctx: &dyn SortContext) -> String {
    let canonical = orient_ordered_expression(expr, ctx);
    let expr = &canonical;
    let subject = subject_key(expr, ctx);
    let family = predicate_family_rank(expr);
    let polarity = polarity_rank(expr);
    let op = operator_rank(expr);
    let payload = payload_key(expr, ctx);
    let structural = structural_key(expr, ctx);
    format!("{subject}\x01{family:03}\x01{polarity:03}\x01{op:03}\x01{payload}\x01{structural}")
}

fn subject_key(expr: &Expr, ctx: &dyn SortContext) -> String {
    match expr.kind() {
        ExprKind::SymVar { origin } => sym_origin_subject_key(origin, ctx),
        ExprKind::RowValue { alias, acquired_integer: true } => format!("00|array-row|{alias}"),
        ExprKind::RowValue { alias, acquired_integer: false } => format!("04|row|{alias}"),
        ExprKind::HeapRef(id) => heap_subject_key(*id, ctx),
        ExprKind::Lit(lit) => literal_subject_key(lit),
        ExprKind::RelationAggregate {
            kind,
            relation,
            select_expr,
            default_expr,
        } => {
            format!(
                "05|relation-agg|{}",
                aggregate_structural_key(*kind, relation, select_expr, default_expr, ctx)
            )
        }
        ExprKind::UnaryOp {
            op: UnaryOpKind::Not,
            operand,
        } => subject_key(operand, ctx),
        ExprKind::UnaryOp { operand, .. } => subject_key(operand, ctx),
        ExprKind::BinOp { op, left, right } => binop_subject_key(*op, left, right, ctx),
        ExprKind::Call { key, args, .. } => call_subject_key(*key, args, ctx),
        ExprKind::CaseWhen { branches } => branches
            .iter()
            .map(|(cond, value)| {
                let c = subject_key(cond, ctx);
                let v = subject_key(value, ctx);
                if c <= v {
                    c
                } else {
                    v
                }
            })
            .min()
            .unwrap_or_else(|| "99|casewhen-empty".to_string()),
        ExprKind::ArrayLit(items) => items
            .iter()
            .map(|item| subject_key(item, ctx))
            .min()
            .unwrap_or_else(|| "99|array-empty".to_string()),
        ExprKind::Relation(relation) => {
            format!("70|relation|{}", relation_structural_key(relation, ctx))
        }
    }
}

fn binop_subject_key(op: BinOpKind, left: &Expr, right: &Expr, ctx: &dyn SortContext) -> String {
    if is_null_comparison(op, left, right) {
        return if left.is_null() {
            subject_key(right, ctx)
        } else {
            subject_key(left, ctx)
        };
    }

    if matches!(
        op,
        BinOpKind::Eq
            | BinOpKind::Ne
            | BinOpKind::Lt
            | BinOpKind::Le
            | BinOpKind::Gt
            | BinOpKind::Ge
    ) {
        let left_key = subject_key(left, ctx);
        let right_key = subject_key(right, ctx);

        if is_literal_like(left) && !is_literal_like(right) {
            return right_key;
        }
        if is_literal_like(right) && !is_literal_like(left) {
            return left_key;
        }
        return if left_key <= right_key {
            left_key
        } else {
            right_key
        };
    }

    let left_key = subject_key(left, ctx);
    let right_key = subject_key(right, ctx);
    if left_key <= right_key {
        left_key
    } else {
        right_key
    }
}

fn call_subject_key(
    key: crate::domains::primitive::registry::FuncKey,
    args: &[Expr],
    ctx: &dyn SortContext,
) -> String {
    let name = func_name(key);

    // Static/opaque call: prefer the first user-facing argument subject if any,
    // but keep the call signature in the subject so it does not collapse into
    // the argument's direct path predicate family.
    for arg in args {
        let k = subject_key(arg, ctx);
        if !is_internal_subject_key(&k) && !k.starts_with("06|literal") {
            return format!("05|call:{name}|arg:{k}");
        }
    }

    format!("05|call:{name}|arity:{}", args.len())
}

fn sym_origin_subject_key(origin: &crate::symvar::SymVarOrigin, ctx: &dyn SortContext) -> String {
    match origin {
        crate::symvar::SymVarOrigin::Parameter { name, .. } => {
            // Boundary/external Pojo projections can be carried as Parameter
            // paths (for example `$e_0.result.name`) after symbolic field
            // expansion.  For display ordering they must stay in the same
            // public E/T subject class as the root HeapRef `$e_0`; otherwise
            // `$e_0.success` sorts before `$e_0 != null` simply because it is
            // represented as Parameter internally.
            if name.starts_with("$e_") || name.starts_with("$t_") || name.starts_with("$h_") {
                public_display_name_subject_key(name)
            } else {
                format!("00|{}", public_path_key(name))
            }
        }
        crate::symvar::SymVarOrigin::External {
            binding_id, path, ..
        } => {
            let root = ctx.external_names().and_then(|names| names.get(binding_id))
                .map(|name| numbered_public_path(name, "$e_"))
                .unwrap_or_else(|| format!("$e_{binding_id:020}"));
            if path.is_empty() {
                format!("01|{root}")
            } else {
                format!("01|{root}.{}", path.join("."))
            }
        }
        crate::symvar::SymVarOrigin::Temporal { binding_id, primitive_type } => {
            let aliases = (matches!(primitive_type.as_str(), "integer" | crate::builder::RECORD_TEMPORAL_TAG)).then(|| ctx.temporal_aliases()).flatten();
            let slot = aliases.and_then(|aliases| aliases.get(binding_id))
                .map_or(*binding_id, |slot| *slot as u64);
            format!("02|$t_{slot:020}")
        }
    }
}

fn heap_subject_key(id: i64, ctx: &dyn SortContext) -> String {
    if let Some(name) = ctx.heap_display_name(id) {
        return public_display_name_subject_key(&name);
    }
    format!("99|heap:{id:020}")
}

fn numbered_public_path(name: &str, prefix: &str) -> String {
    let Some(rest) = name.strip_prefix(prefix) else { return name.into(); };
    let (root, suffix) = rest.split_once('.').map_or((rest, String::new()),
        |(root, path)| (root, format!(".{path}")));
    root.parse::<u64>().map_or_else(|_| name.into(),
        |slot| format!("{prefix}{slot:020}{suffix}"))
}

fn public_display_name_subject_key(name: &str) -> String {
    if name.starts_with("$e_") {
        format!("01|{}", numbered_public_path(name, "$e_"))
    } else if name.starts_with("$t_") {
        format!("02|{}", numbered_public_path(name, "$t_"))
    } else if name.starts_with("$h_") {
        format!("99|{name}")
    } else {
        // Heap display names such as `request` / `family.name` are user-facing
        // parameter-style paths unless a caller later provides a richer context.
        format!("00|{}", public_path_key(name))
    }
}

fn literal_subject_key(lit: &Lit) -> String {
    format!("06|literal|{}", literal_payload_key(lit))
}

fn predicate_family_rank(expr: &Expr) -> u8 {
    match expr.kind() {
        ExprKind::UnaryOp {
            op: UnaryOpKind::Not,
            operand,
        } => predicate_family_rank(operand),
        ExprKind::BinOp { op, left, right } => {
            if matches!(op, BinOpKind::And | BinOpKind::Or) {
                return predicate_family_rank(left).min(predicate_family_rank(right));
            }

            if is_null_comparison(*op, left, right) {
                return 0;
            }
            if matches!(op, BinOpKind::Eq | BinOpKind::Ne) {
                return 4;
            }
            if matches!(
                op,
                BinOpKind::Lt | BinOpKind::Le | BinOpKind::Gt | BinOpKind::Ge
            ) {
                if contains_aggregate(left) || contains_aggregate(right) {
                    return 7;
                }
                return 5;
            }
            10
        }
        ExprKind::Call { .. } => 8,
        ExprKind::RelationAggregate { .. } => 7,
        _ => 10,
    }
}

fn polarity_rank(expr: &Expr) -> u8 {
    match expr.kind() {
        ExprKind::UnaryOp {
            op: UnaryOpKind::Not,
            ..
        } => 1,
        ExprKind::BinOp {
            op: BinOpKind::Ne, ..
        } => 1,
        _ => 0,
    }
}

fn operator_rank(expr: &Expr) -> u8 {
    match expr.kind() {
        ExprKind::UnaryOp {
            op: UnaryOpKind::Not,
            operand,
        } => operator_rank(operand),
        ExprKind::BinOp { op, .. } => match op {
            BinOpKind::Eq => 0,
            BinOpKind::Ne => 1,
            BinOpKind::Lt => 2,
            BinOpKind::Le => 3,
            BinOpKind::Gt => 4,
            BinOpKind::Ge => 5,
            BinOpKind::And => 10,
            BinOpKind::Or => 11,
            BinOpKind::Add => 20,
            BinOpKind::Sub => 21,
            BinOpKind::Mul => 22,
        },
        _ => 99,
    }
}

fn payload_key(expr: &Expr, ctx: &dyn SortContext) -> String {
    match expr.kind() {
        ExprKind::UnaryOp {
            op: UnaryOpKind::Not,
            operand,
        } => payload_key(operand, ctx),
        ExprKind::BinOp { op, left, right } => {
            let subject = binop_subject_key(*op, left, right, ctx);
            let left_subject = subject_key(left, ctx);
            let right_subject = subject_key(right, ctx);

            if left_subject == subject && right_subject != subject {
                structural_key(right, ctx)
            } else if right_subject == subject && left_subject != subject {
                structural_key(left, ctx)
            } else {
                let l = structural_key(left, ctx);
                let r = structural_key(right, ctx);
                if l <= r {
                    format!("{l}|{r}")
                } else {
                    format!("{r}|{l}")
                }
            }
        }
        ExprKind::Call { key, args, .. } => {
            let mut parts = vec![func_name(*key)];
            parts.extend(args.iter().map(|a| structural_key(a, ctx)));
            parts.join("|")
        }
        ExprKind::Lit(lit) => literal_payload_key(lit),
        _ => structural_key(expr, ctx),
    }
}

fn structural_key(expr: &Expr, ctx: &dyn SortContext) -> String {
    match expr.kind() {
        ExprKind::Lit(lit) => format!("lit:{}", literal_payload_key(lit)),
        ExprKind::SymVar { origin } => {
            use crate::symvar::SymVarOrigin::*;
            let primitive_type = match origin {
                Parameter { primitive_type, .. }
                | External { primitive_type, .. }
                | Temporal { primitive_type, .. } => primitive_type,
            };
            key_node("sym", vec![sym_origin_subject_key(origin, ctx), primitive_type.clone()])
        }
        ExprKind::RowValue { alias, acquired_integer } => format!("row:{acquired_integer}:{alias}"),
        ExprKind::HeapRef(id) => format!("heap:{}", heap_subject_key(*id, ctx)),
        ExprKind::UnaryOp { op, operand } => {
            format!("unary:{op:?}({})", structural_key(operand, ctx))
        }
        ExprKind::BinOp { op, left, right } => {
            let (op, swap) = ordered_comparison_view(*op, left, right, ctx).unwrap_or((*op, false));
            let (left, right) = if swap { (right, left) } else { (left, right) };
            format!(
                "bin:{op:?}({},{})",
                structural_key(left, ctx),
                structural_key(right, ctx)
            )
        }
        ExprKind::Call { key, args, .. } => {
            let args = args
                .iter()
                .map(|a| structural_key(a, ctx))
                .collect::<Vec<_>>()
                .join(",");
            format!("call:{}({args})", func_name(*key))
        }
        ExprKind::CaseWhen { branches } => {
            let branches = branches
                .iter()
                .map(|(c, v)| format!("{}=>{}", structural_key(c, ctx), structural_key(v, ctx)))
                .collect::<Vec<_>>()
                .join(";");
            format!("case:{branches}")
        }
        ExprKind::ArrayLit(items) => {
            let items = items
                .iter()
                .map(|i| structural_key(i, ctx))
                .collect::<Vec<_>>()
                .join(",");
            format!("array:{items}")
        }
        ExprKind::RelationAggregate {
            kind,
            relation,
            select_expr,
            default_expr,
        } => {
            format!(
                "relagg:{}",
                aggregate_structural_key(*kind, relation, select_expr, default_expr, ctx)
            )
        }
        ExprKind::Relation(relation) => {
            format!("relation:{}", relation_structural_key(relation, ctx))
        }
    }
}

// Relation keys must recurse through Expr rather than its
// Debug representation (which contains FuncKey/ExprProfile implementation IDs).
// Length-prefix every field and preserve child/event order; this is structural
// encoding, not a new commutativity rule for relations or event histories.
fn key_node(tag: &str, fields: Vec<String>) -> String {
    let mut out = format!("{tag}:{}:", fields.len());
    for field in fields {
        out.push_str(&format!("{}:{field}", field.len()));
    }
    out
}
fn optional_expr_key(expr: &Option<Expr>, ctx: &dyn SortContext) -> String {
    match expr {
        None => "none".into(),
        Some(value) => key_node("some", vec![structural_key(value, ctx)]),
    }
}
fn aggregate_structural_key(
    kind: RelationAggregateKind,
    relation: &crate::domains::relation::RelationExpr,
    select: &Option<Expr>,
    default: &Option<Expr>,
    ctx: &dyn SortContext,
) -> String {
    key_node(
        "aggregate",
        vec![
            aggregate_kind_rank(kind).to_string(),
            relation_structural_key(relation, ctx),
            optional_expr_key(select, ctx),
            optional_expr_key(default, ctx),
        ],
    )
}
fn relation_structural_key(
    relation: &crate::domains::relation::RelationExpr,
    ctx: &dyn SortContext,
) -> String {
    use crate::domains::relation::RelationExpr as R;
    match relation {
        R::Empty => key_node("empty", vec![]),
        R::Base { name, alias } => key_node("base", vec![name.clone(), alias.clone()]),
        R::Vector { value, alias } => {
            key_node("vector", vec![structural_key(value, ctx), alias.clone()])
        }
        R::Select { predicate, input } => key_node(
            "select",
            vec![
                structural_key(predicate, ctx),
                relation_structural_key(input, ctx),
            ],
        ),
        R::Project {
            value,
            alias,
            input,
        } => key_node(
            "project",
            vec![
                structural_key(value, ctx),
                alias.clone(),
                relation_structural_key(input, ctx),
            ],
        ),
        R::Distinct(input) => key_node("distinct", vec![relation_structural_key(input, ctx)]),
        R::Union(branches) => key_node("union", branches.iter().map(|branch| relation_structural_key(branch, ctx)).collect()),
    }
}

/// Canonical operand direction for an existing ordered predicate. Swapping
/// operands reverses the operator; it does not complement the predicate, widen
/// its definition domain, or reorder source effects. Equal-strength operands
/// use the same context-sensitive structural key as all other peer terms.
pub fn ordered_comparison_view(
    op: BinOpKind,
    left: &Expr,
    right: &Expr,
    ctx: &dyn SortContext,
) -> Option<(BinOpKind, bool)> {
    if !matches!(
        op,
        BinOpKind::Lt | BinOpKind::Le | BinOpKind::Gt | BinOpKind::Ge
    ) {
        return None;
    }
    let ls = canonical_order_subject_strength(left, ctx);
    let rs = canonical_order_subject_strength(right, ctx);
    let swap =
        rs > ls || (rs == ls && canonical_order_key(right, ctx) < canonical_order_key(left, ctx));
    Some((if swap { op.swapped_comparison()? } else { op }, swap))
}

pub(crate) fn orient_ordered_expression(expr: &Expr, ctx: &dyn SortContext) -> Expr {
    if let ExprKind::BinOp { op, left, right } = expr.kind() {
        if let Some((op, true)) = ordered_comparison_view(*op, left, right, ctx) {
            return Expr::new(ExprKind::BinOp {
                op,
                left: right.clone(),
                right: left.clone(),
            });
        }
    }
    expr.clone()
}

fn canonical_order_orient_ordered_predicate_to_rhs(
    op: BinOpKind,
    left: &Expr,
    right: &Expr,
    ctx: &dyn SortContext,
) -> bool {
    ordered_comparison_view(op, left, right, ctx).is_some_and(|(_, swap)| swap)
}

fn canonical_order_orient_equality_to_rhs(left: &Expr, right: &Expr, ctx: &dyn SortContext) -> bool {
    let left_strength = canonical_order_subject_strength(left, ctx);
    let right_strength = canonical_order_subject_strength(right, ctx);

    if right_strength > left_strength {
        return true;
    }
    if left_strength > right_strength {
        return false;
    }

    canonical_order_key(right, ctx) < canonical_order_key(left, ctx)
}

fn canonical_order_subject_strength(expr: &Expr, ctx: &dyn SortContext) -> u8 {
    match expr.kind() {
        ExprKind::Lit(_) | ExprKind::HeapRef(_) | ExprKind::RowValue { .. } => 10,
        ExprKind::RelationAggregate { .. } => 90,
        ExprKind::Call { .. } => 40,
        ExprKind::SymVar { origin } => match origin {
            crate::symvar::SymVarOrigin::Parameter { name, .. } => {
                if name.contains('.') || name.contains('[') {
                    100
                } else {
                    70
                }
            }
            crate::symvar::SymVarOrigin::External { path, .. } => {
                if path.is_empty() {
                    65
                } else {
                    100
                }
            }
            crate::symvar::SymVarOrigin::Temporal { .. } => 65,
        },
        ExprKind::BinOp { .. } | ExprKind::CaseWhen { .. } => {
            // Compound predicates do not become stronger semantic subjects just
            // because their internal printed form is longer.  Their stable
            // position is decided by the display compare key.
            let _ = ctx;
            40
        }
        ExprKind::UnaryOp { operand, .. } => canonical_order_subject_strength(operand, ctx),
        ExprKind::ArrayLit(_) => 40,
        ExprKind::Relation(_) => 45,
    }
}

fn literal_payload_key(lit: &Lit) -> String {
    match lit {
        Lit::Null => "0:null".to_string(),
        Lit::Bool(v) => format!("1:{v}"),
        Lit::Int(v) => format!("2:{v:020}"),
        Lit::BigInt(v) => format!("2:{v:020}"),
        Lit::Str(v) => format!("3:{v}"),
    }
}

fn public_path_key(path: &str) -> String {
    path.to_string()
}

fn is_literal_like(expr: &Expr) -> bool {
    matches!(expr.kind(), ExprKind::Lit(_))
}

fn is_internal_subject_key(key: &str) -> bool {
    key.starts_with("99|")
}

fn is_null_comparison(op: BinOpKind, left: &Expr, right: &Expr) -> bool {
    matches!(op, BinOpKind::Eq | BinOpKind::Ne) && (left.is_null() || right.is_null())
}

fn contains_aggregate(expr: &Expr) -> bool {
    match expr.kind() {
        ExprKind::RelationAggregate { .. } => true,
        ExprKind::UnaryOp { operand, .. } => contains_aggregate(operand),
        ExprKind::BinOp { left, right, .. } => {
            contains_aggregate(left) || contains_aggregate(right)
        }
        ExprKind::Call { args, .. } => args.iter().any(contains_aggregate),
        ExprKind::CaseWhen { branches } => branches
            .iter()
            .any(|(cond, value)| contains_aggregate(cond) || contains_aggregate(value)),
        ExprKind::ArrayLit(items) => items.iter().any(contains_aggregate),
        _ => false,
    }
}

fn aggregate_kind_rank(kind: RelationAggregateKind) -> u8 {
    match kind {
        RelationAggregateKind::Count => 0,
        RelationAggregateKind::Sum => 1,
        RelationAggregateKind::FirstValue => 3,
        RelationAggregateKind::NthValue => 4,
        RelationAggregateKind::Exists => 5,
    }
}
