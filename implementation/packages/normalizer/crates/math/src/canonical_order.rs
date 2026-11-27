//! 统一正则规范 (Unified Canonical Form): shared structural ordering.
//!
//! This module is the shared `expr.compareTo(ctx)` foundation.
//! Callers may use it only after they have proven the surrounding expression
//! family is commutative/associative (for example AND/OR).  It is not a license
//! to reorder CaseWhen arms, side-effect ledgers, arguments, list elements, or
//! string concatenation.
//!
//! The design contract is `design/chapters/第八章 · 统一定形规范.md`. The shared key is:
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
            if stronger_comparison_subject_is_right(left, right, ctx) {
                Some((op, true))
            } else {
                None
            }
        }
        _ => ordered_comparison_view(op, left, right, ctx).filter(|&(_, swap)| swap),
    }
}

/// Deterministic display-order key for peer terms in a proven commutative family.
pub fn canonical_order_key(expr: &Expr, ctx: &dyn SortContext) -> String {
    // Expr is immutable. Without scoped heap, environment or temporal names this key is
    // independent of SortContext, so the bounded cache is semantics-preserving and
    // avoids recomputing the same DisplayIR key in high-frequency AND/OR sorts.
    if !expr.has_heap_ref() && ctx.temporal_aliases().is_none() && ctx.external_names().is_none() {
        if let Some(cached) = CANONICAL_ORDER_CACHE.with(|cache| cache.borrow().get(expr).cloned()) {
            return cached;
        }
        let out = canonical_order_key_uncached(expr, ctx);
        let cache_weight = expr.size();
        crate::cache::with_entry_budget(cache_weight, || {
            CANONICAL_ORDER_CACHE.with(|cache| {
                let mut cache = cache.borrow_mut();
                // Preserve the ordering table's historical one-key offset.
                crate::cache::insert_bounded(&mut cache, expr.clone(), out.clone(),
                    crate::cache::DEFAULT_KEY_LIMIT + 1, "ordering_axiom");
            });
        });
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
        ExprKind::UnaryOp { operand, .. } => subject_key(operand, ctx),
        ExprKind::BinOp { op, left, right } => binop_subject_key(*op, left, right, ctx),
        ExprKind::Call { key, args, .. } => call_subject_key(*key, args, ctx),
        ExprKind::CaseWhen { branches } => branches
            .iter()
            .map(|(cond, value)| subject_key(cond, ctx).min(subject_key(value, ctx)))
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
    use BinOpKind::*;
    if matches!(op, Eq | Ne | Lt | Le | Gt | Ge) {
        let subject_rank = |value: &Expr| match value.as_lit() {
            Some(Lit::Null) if matches!(op, Eq | Ne) => 2,
            Some(_) => 1,
            None => 0,
        };
        let left_rank = subject_rank(left);
        let right_rank = subject_rank(right);
        if left_rank != right_rank {
            let subject = if left_rank < right_rank { left } else { right };
            return subject_key(subject, ctx);
        }
    }
    subject_key(left, ctx).min(subject_key(right, ctx))
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
        crate::symvar::SymVarOrigin::Parameter { name, .. } => format!("00|{name}"),
        crate::symvar::SymVarOrigin::External {
            binding_id, path, ..
        } => {
            let root = ctx.external_names().and_then(|names| names.get(binding_id))
                .map(|name| numbered_receipt_root(name))
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
        return format!("00|{name}");
    }
    format!("99|heap:{id:020}")
}

// Published receipt names are ordinal roots; the typed External path is
// appended separately. A named input/state remains its ordinary public path.
fn numbered_receipt_root(name: &str) -> String {
    if let Some(slot) = name.strip_prefix("$e_").and_then(|root| root.parse::<u64>().ok()) {
        format!("$e_{slot:020}")
    } else {
        name.to_string()
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
                if left.contains_relation_aggregate() || right.contains_relation_aggregate() {
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
            } else if left_subject != subject {
                // binop_subject_key selects one of the two subject keys, so
                // a nonmatching left key means the right key is the subject.
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
        ExprKind::SymVar { origin } => key_node(
            "sym",
            vec![sym_origin_subject_key(origin, ctx), origin.primitive_type().to_owned()],
        ),
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
        R::Grouped(group) => key_node("group", vec![
            group.table().into(), key_node("columns", group.columns().map(str::to_owned).collect()),
        ]),
        R::ProjectedUnion(columns) => key_node("projected-union", columns.iter()
            .map(|(table, column)| key_node("column", vec![table.into(), column.into()])).collect()),
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
    let swap = stronger_comparison_subject_is_right(left, right, ctx);
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

fn stronger_comparison_subject_is_right(left: &Expr, right: &Expr, ctx: &dyn SortContext) -> bool {
    canonical_order_subject_strength(left)
        .cmp(&canonical_order_subject_strength(right))
        .then_with(|| canonical_order_key(right, ctx).cmp(&canonical_order_key(left, ctx)))
        .is_lt()
}

fn canonical_order_subject_strength(expr: &Expr) -> u8 {
    match expr.kind() {
        ExprKind::Lit(_) | ExprKind::HeapRef(_) | ExprKind::RowValue { .. } => 10,
        ExprKind::RelationAggregate { .. } => 90,
        ExprKind::Call { .. } => 40,
        ExprKind::SymVar { origin } => match origin {
            crate::symvar::SymVarOrigin::Parameter { name, .. } => {
                // Indexed observations are relation expressions, not names
                // of scalar parameter paths.
                if name.contains('.') {
                    100
                } else {
                    70
                }
            }
            crate::symvar::SymVarOrigin::External { path, .. } if !path.is_empty() => 100,
            crate::symvar::SymVarOrigin::External { .. }
            | crate::symvar::SymVarOrigin::Temporal { .. } => 65,
        },
        // Compound expressions do not become stronger subjects because their
        // printed form is longer. Peers are ordered by the display compare key.
        ExprKind::BinOp { .. } | ExprKind::CaseWhen { .. } | ExprKind::ArrayLit(_) => 40,
        ExprKind::UnaryOp { operand, .. } => canonical_order_subject_strength(operand),
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

fn is_internal_subject_key(key: &str) -> bool {
    key.starts_with("99|")
}

fn is_null_comparison(op: BinOpKind, left: &Expr, right: &Expr) -> bool {
    matches!(op, BinOpKind::Eq | BinOpKind::Ne) && (left.is_null() || right.is_null())
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

#[cfg(test)]
mod typed_subject_order_tests {
    use super::*;
    use crate::{builder as b, DefaultSortContext, Primitive, ScalarSymbolType};
    use crate::domains::primitive::registry::FuncSemantics;
    use crate::domains::relation::{relation_count, RelationExpr};

    #[test]
    fn literal_operand_does_not_choose_the_subject_or_nested_orientation() {
        let x = b::param("x", ScalarSymbolType::Integer);
        let point = b::lit_int(3);
        let right_subject = b::lt(point.clone(), x.clone());
        let left_subject = b::gt(x.clone(), point.clone());

        assert_eq!(
            binop_subject_key(BinOpKind::Lt, &point, &x, &DefaultSortContext),
            subject_key(&x, &DefaultSortContext)
        );
        assert_eq!(
            structural_key(&b::not(right_subject), &DefaultSortContext),
            structural_key(&b::not(left_subject), &DefaultSortContext)
        );
        assert_ne!(
            structural_key(&b::not(b::lt(x, point)), &DefaultSortContext),
            structural_key(&b::not(b::lit_bool(true)), &DefaultSortContext)
        );
    }

    #[test]
    fn optional_and_relational_predicate_families_accept_both_orientations() {
        let x = b::param("x", ScalarSymbolType::Integer);
        let count = relation_count(RelationExpr::base("offers", "offer"));
        let nullable = b::heap_ref(-7);
        assert_eq!(predicate_family_rank(&b::lt(x.clone(), count.clone())), 7);
        assert_eq!(predicate_family_rank(&b::gt(count, x.clone())), 7);
        assert_eq!(predicate_family_rank(&b::lt(x, b::lit_int(2))), 5);
        assert_eq!(predicate_family_rank(&b::eq(b::lit_null(), nullable.clone())), 0);
        assert_eq!(predicate_family_rank(&b::ne(nullable, b::lit_null())), 0);
    }

    #[test]
    fn internal_call_arguments_do_not_displace_the_first_public_subject() {
        let marker = b::array_lit(vec![]);
        let x = b::param("x", ScalarSymbolType::Integer);
        let key = crate::register_func(
            "test.order.vector_and_threshold",
            vec![Primitive::Composite, Primitive::Number],
            Some(Primitive::Boolean),
            FuncSemantics::pure_function(),
        );
        assert_eq!(
            call_subject_key(key, &[marker, x], &DefaultSortContext),
            "05|call:test.order.vector_and_threshold|arg:00|x"
        );
    }

    #[test]
    fn acquired_receipt_field_and_bare_scalar_have_distinct_subject_strength() {
        let x = b::param("x", ScalarSymbolType::Integer);
        let receipt = b::sym_e(0, ScalarSymbolType::Integer);
        let field = b::sym_e_path(1, vec!["score".to_owned()], ScalarSymbolType::Integer);

        assert_eq!(ordered_comparison_view(BinOpKind::Lt, &x, &field, &DefaultSortContext),
            Some((BinOpKind::Gt, true)));
        assert_eq!(ordered_comparison_view(BinOpKind::Gt, &field, &x, &DefaultSortContext),
            Some((BinOpKind::Gt, false)));
        assert_eq!(ordered_comparison_view(BinOpKind::Lt, &x, &receipt, &DefaultSortContext),
            Some((BinOpKind::Lt, false)));
    }
}
