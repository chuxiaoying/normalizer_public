//! Relation-valued Expr local simplification.
//!
//! RCL lives inside the single math `Expr` IR.  This module only simplifies
//! relation aggregate queries that are already represented as `ExprKind` nodes;
//! it must not introduce parallel Bool/Count expression systems.

use crate::builder;
use crate::domains::relation::RelationExpr;
use crate::expr::{Expr, ExprKind, RelationAggregateKind};
use std::time::Instant;

/// Admit only a bare Count with a compact integer bound. Both source
/// orientations share the comparison owner and cannot certify projections.
pub(super) fn relation_count_bound(expr: &Expr) -> Option<(crate::BinOpKind, &RelationExpr, i64)> {
    let (op, count, bound) =
        super::constraint::comparison_literal_rhs(expr.as_binop()?, super::constraint::compact_integer)?;
    let (RelationAggregateKind::Count, relation, None, None) = count.as_relation_aggregate()?
    else {
        return None;
    };
    Some((op, relation, bound))
}

/// Exact cardinality is nonnegative and is zero exactly for an empty relation.
fn rewrite_count_zero_comparison(expr: &Expr) -> Option<Expr> {
    use crate::BinOpKind::*;
    let (op, relation, 0) = relation_count_bound(expr)? else {
        return None;
    };
    let exists = || crate::domains::relation::relation_exists(relation.clone());
    Some(match op {
        Eq | Le => builder::not(exists()),
        Ne | Gt => exists(),
        Lt => builder::lit_bool(false),
        Ge => builder::lit_bool(true),
        _ => return None,
    })
}

/// An empty relation has no rows to count, sum, test, or select.
fn rewrite_empty_aggregate(expr: &Expr) -> Option<Expr> {
    use RelationAggregateKind::*;
    let (kind, RelationExpr::Empty, _, default) = expr.as_relation_aggregate()? else {
        return None;
    };
    match kind {
        Count | Sum => Some(builder::lit_int(0)),
        Exists => Some(builder::lit_bool(false)),
        FirstValue => default.cloned(),
        NthValue => None,
    }
}

/// The zero/null ordered-row observation has one representative: FIRST of
/// that same row. NthValue's constructor requires OrderedList semantics at
/// its host; this rewrite neither invents order nor changes the relation.
/// Only layouts with a certified single output binder are admitted here.
fn rewrite_index_zero_as_first(expr: &Expr) -> Option<Expr> {
    let (RelationAggregateKind::NthValue, relation, Some(index), Some(default)) =
        expr.as_relation_aggregate()?
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
    Some(crate::domains::relation::relation_first_value(
        relation.clone(),
        builder::row_value(alias),
        default.clone(),
    ))
}

/// Apply local relation-query rewrite rules at the Expr layer.
///
/// These rules operate over structured `RelationExpr` aggregate nodes; the
/// surrounding arithmetic/boolean structure remains the existing math `Expr`
/// language.
pub(crate) fn simplify(expr: &Expr) -> Expr {
    let profile = if crate::profile::enabled() {
        expr.as_relation_aggregate().map(|(kind, relation, select_expr, _)| {
            let shape = relation_profile_shape(relation);
            (kind, relation, select_expr, shape, Instant::now())
        })
    } else {
        None
    };
    let output = rewrite_count_zero_comparison(expr)
        .or_else(|| rewrite_empty_aggregate(expr))
        .or_else(|| rewrite_index_zero_as_first(expr))
        .unwrap_or_else(|| expr.clone());
    if let Some((kind, relation, select_expr, (input_size, input_depth), start)) = profile {
        let output_relation =
            output.as_relation_aggregate().map(|(_, relation, _, _)| relation);
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
        RelationExpr::Select { predicate: value, input }
        | RelationExpr::Project { value, input, .. } => {
            let (size, depth) = relation_profile_shape(input);
            (1 + value.size() + size, 1 + depth)
        }
        RelationExpr::Distinct(input) => {
            let (size, depth) = relation_profile_shape(input);
            (1 + size, 1 + depth)
        }
        RelationExpr::Union(branches) => branches.iter().fold((1, 1), |(size, depth), branch| {
            let (s, d) = relation_profile_shape(branch);
            (size + s, depth.max(1 + d))
        }),
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
        RelationExpr::Union(_) => "union",
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

fn projection_kind(select_expr: Option<&Expr>) -> &'static str {
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
