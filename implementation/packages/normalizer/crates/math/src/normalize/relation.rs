//! Relation-valued Expr local simplification.
//!
//! RCL lives inside the single math `Expr` IR.  This module only simplifies
//! relation aggregate queries that are already represented as `ExprKind` nodes;
//! it must not introduce parallel Bool/Count expression systems.

use crate::builder;
use crate::domains::relation::RelationExpr;
use crate::expr::{Expr, ExprKind, RelationAggregateKind};
use std::time::Instant;

fn relation_aggregate_fields(
    expr: &Expr,
) -> Option<(
    RelationAggregateKind,
    &RelationExpr,
    &Option<Expr>,
    &Option<Expr>,
)> {
    let ExprKind::RelationAggregate {
        kind,
        relation,
        select_expr,
        default_expr,
    } = expr.kind()
    else {
        return None;
    };
    Some((*kind, relation.as_ref(), select_expr, default_expr))
}

fn relation_aggregate_expr(
    kind: RelationAggregateKind,
    relation: RelationExpr,
    select_expr: Option<Expr>,
    default_expr: Option<Expr>,
) -> Expr {
    Expr::new(ExprKind::RelationAggregate {
        kind,
        relation: Box::new(relation),
        select_expr,
        default_expr,
    })
}

/// Exact cardinality is nonnegative and is zero exactly for an empty relation.
/// This does not inspect signed_word/count-distinct/sum wrappers: overflow and
/// projection semantics cannot be inferred from their printed shape.
fn rewrite_count_zero_comparison(expr: &Expr) -> Option<Expr> {
    use crate::BinOpKind::*;
    let (mut op, left, right) = expr.as_binop()?;
    let query = if right.as_lit() == Some(&crate::Lit::Int(0)) {
        left
    } else if left.as_lit() == Some(&crate::Lit::Int(0)) {
        op = match op {
            Lt => Gt,
            Le => Ge,
            Gt => Lt,
            Ge => Le,
            Eq => Eq,
            Ne => Ne,
            _ => return None,
        };
        right
    } else {
        return None;
    };
    let (RelationAggregateKind::Count, relation, None, None) = relation_aggregate_fields(query)?
    else {
        return None;
    };
    let exists = crate::domains::relation::relation_exists(relation.clone());
    Some(match op {
        Eq | Le => builder::not(exists),
        Ne | Gt => exists,
        Lt => builder::lit_bool(false),
        Ge => builder::lit_bool(true),
        _ => return None,
    })
}

/// Counting or exactly summing an empty relation yields zero.
fn rewrite_empty_count_or_sum(expr: &Expr) -> Option<Expr> {
    match relation_aggregate_fields(expr) {
        Some((
            RelationAggregateKind::Count | RelationAggregateKind::Sum,
            RelationExpr::Empty,
            _,
            _,
        )) => Some(builder::lit_int(0)),
        _ => None,
    }
}

/// The zero/null ordered-row observation has one representative: FIRST of
/// that same row. NthValue's constructor requires OrderedList semantics at
/// its host; this rewrite neither invents order nor changes the relation.
/// Only layouts with a certified single output binder are admitted here.
fn rewrite_index_zero_as_first(expr: &Expr) -> Option<Expr> {
    let (RelationAggregateKind::NthValue, relation, Some(index), Some(default)) =
        relation_aggregate_fields(expr)?
    else {
        return None;
    };
    if !matches!(index.as_lit(), Some(crate::Lit::Int(0))) || !default.is_null() {
        return None;
    }
    fn ordered_row_alias(relation: &RelationExpr) -> Option<&str> {
        match relation {
            RelationExpr::Base { alias, .. } => Some(alias),
            RelationExpr::Select { input, .. } => ordered_row_alias(input),
            // Distinct has no stable-order declaration. Project has its
            // own composition rule; multi-input and MapKv layouts cannot
            // borrow a guessed alias from one of their inputs.
            _ => None,
        }
    }
    let alias = ordered_row_alias(relation)?;
    Some(relation_aggregate_expr(
        RelationAggregateKind::FirstValue,
        relation.clone(),
        Some(builder::row_value(alias)),
        Some(default.clone()),
    ))
}

/// Rule Q2: Exists(Empty) = false.
///
/// Local simplification: Empty has no rows, so existence is false regardless of
/// aggregate payload. This is expressed as the existing math boolean literal,
/// not as a parallel BoolExpr.
fn rewrite_empty_exists(expr: &Expr) -> Option<Expr> {
    match relation_aggregate_fields(expr) {
        Some((RelationAggregateKind::Exists, RelationExpr::Empty, _, _)) => {
            Some(builder::lit_bool(false))
        }
        _ => None,
    }
}

fn rewrite_empty_first(expr: &Expr) -> Option<Expr> {
    match relation_aggregate_fields(expr) {
        Some((RelationAggregateKind::FirstValue, RelationExpr::Empty, _, Some(default_expr))) => {
            Some(default_expr.clone())
        }
        _ => None,
    }
}

/// Rule Q3b: NthValue(Empty, i, default) = default.
///
/// Local simplification: an empty ordered list has no indexed row.
fn rewrite_empty_index(expr: &Expr) -> Option<Expr> {
    match relation_aggregate_fields(expr) {
        Some((RelationAggregateKind::NthValue, RelationExpr::Empty, _, Some(default_expr))) => {
            Some(default_expr.clone())
        }
        _ => None,
    }
}

/// Apply local relation-query rewrite rules at the Expr layer.
///
/// These rules operate over structured `RelationExpr` aggregate nodes; the
/// surrounding arithmetic/boolean structure remains the existing math `Expr`
/// language.
pub(crate) fn simplify(expr: &Expr) -> Expr {
    let profile = if crate::profile::enabled() {
        relation_aggregate_fields(expr).map(|(kind, relation, select_expr, _)| {
            let shape = relation_profile_shape(relation);
            (kind, relation, select_expr, shape, Instant::now())
        })
    } else {
        None
    };
    let output = rewrite_count_zero_comparison(expr)
        .or_else(|| rewrite_empty_count_or_sum(expr))
        .or_else(|| rewrite_index_zero_as_first(expr))
        .or_else(|| rewrite_empty_exists(expr))
        .or_else(|| rewrite_empty_first(expr))
        .or_else(|| rewrite_empty_index(expr))
        .unwrap_or_else(|| expr.clone());
    if let Some((kind, relation, select_expr, (input_size, input_depth), start)) = profile {
        let output_relation =
            relation_aggregate_fields(&output).map(|(_, relation, _, _)| relation);
        let (output_size, output_depth) = output_relation
            .map(relation_profile_shape)
            .unwrap_or((0, 0));
        let input_node_kind = relation_node_kind(relation);
        let output_node_kind = output_relation.map(relation_node_kind).unwrap_or("scalar");
        let projection = projection_kind(select_expr);
        let skip_reason = (output == *expr).then_some("no_rewrite");
        crate::profile::rule_event(
            "math.relation.scope",
            input_size,
            output_size,
            start.elapsed().as_micros(),
            skip_reason,
            &[
                (
                    "relation_kind",
                    crate::profile::json_string(relation_kind(kind)),
                ),
                ("relation_input_size", input_size.to_string()),
                ("relation_input_depth", input_depth.to_string()),
                ("relation_output_size", output_size.to_string()),
                ("relation_output_depth", output_depth.to_string()),
                (
                    "relation_node_kind",
                    crate::profile::json_string(input_node_kind),
                ),
                (
                    "relation_output_node_kind",
                    crate::profile::json_string(output_node_kind),
                ),
                ("projection", crate::profile::json_string(projection)),
                ("rewritten", (output != *expr).to_string()),
            ],
        );
    }
    output
}

fn relation_profile_shape(relation: &RelationExpr) -> (usize, usize) {
    match relation {
        RelationExpr::Empty | RelationExpr::Base { .. } => (1, 1),
        RelationExpr::Vector { value, .. } => (1 + value.size(), 1 + value.depth()),
        RelationExpr::Select { predicate, input } => {
            let (size, depth) = relation_profile_shape(input);
            (1 + predicate.size() + size, 1 + depth)
        }
        RelationExpr::Project { value, input, .. } => {
            let (size, depth) = relation_profile_shape(input);
            (1 + value.size() + size, 1 + depth)
        }
        RelationExpr::Distinct(input) => {
            let (size, depth) = relation_profile_shape(input);
            (1 + size, 1 + depth)
        }
    }
}

fn relation_node_kind(relation: &RelationExpr) -> &'static str {
    match relation {
        RelationExpr::Empty => "empty",
        RelationExpr::Base { .. } => "base",
        RelationExpr::Vector { .. } => "vector",
        RelationExpr::Select { .. } => "select",
        RelationExpr::Project { .. } => "project",
        RelationExpr::Distinct(_) => "distinct",
    }
}

fn relation_kind(kind: RelationAggregateKind) -> &'static str {
    match kind {
        RelationAggregateKind::Count => "count",
        RelationAggregateKind::Sum => "sum",
        RelationAggregateKind::FirstValue => "first_value",
        RelationAggregateKind::NthValue => "nth_value",
        RelationAggregateKind::Exists => "exists",
    }
}

fn projection_kind(select_expr: &Option<Expr>) -> &'static str {
    let Some(select_expr) = select_expr else {
        return "none";
    };
    match select_expr.kind() {
        ExprKind::RowValue { .. } => "row_value",
        ExprKind::SymVar { .. } => "parameter",
        ExprKind::Lit(_) => "literal",
        _ => "expression",
    }
}
