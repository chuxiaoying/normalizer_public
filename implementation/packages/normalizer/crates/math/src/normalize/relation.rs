//! Relation-valued Expr local simplification.
//!
//! RCL lives inside the single math `Expr` IR.  This module only simplifies
//! relation aggregate queries that are already represented as `ExprKind` nodes;
//! it must not introduce parallel Bool/Count expression systems.

use crate::builder;
use crate::domains::relation::RelationExpr;
use crate::expr::{Expr, RelationAggregateKind};
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

/// Each contributing row is exactly zero, and the sum's empty identity is
/// zero too. Acquisitions remain owned by the execution layer, not the sum.
fn rewrite_zero_sum(expr: &Expr) -> Option<Expr> {
    let (RelationAggregateKind::Sum, _, Some(value), None) = expr.as_relation_aggregate()? else {
        return None;
    };
    (value.as_lit() == Some(&crate::Lit::Int(0))).then(|| builder::lit_int(0))
}

/// The zero/null ordered-row observation has one representative: FIRST of
/// that same row. NthValue's constructor requires OrderedList semantics at
/// its host; this rewrite neither invents order nor changes the relation.
/// Only layouts with a certified single output binder are admitted here.
fn rewrite_index_zero_as_first(expr: &Expr) -> Option<Expr> {
    // A bare base is the only admitted layout with a certified output binder.
    let (
        RelationAggregateKind::NthValue,
        relation @ RelationExpr::Base { alias, .. },
        Some(index),
        _,
    ) = expr.as_relation_aggregate()?
    else {
        return None;
    };
    if !matches!(index.as_lit(), Some(crate::Lit::Int(0))) {
        return None;
    }
    Some(crate::domains::relation::relation_first_value(
        relation.clone(),
        builder::row_value(alias),
        builder::lit_null(),
    ))
}

/// Apply local relation-query rewrite rules at the Expr layer.
///
/// These rules operate over structured `RelationExpr` aggregate nodes; the
/// surrounding arithmetic/boolean structure remains the existing math `Expr`
/// language.
pub(crate) fn simplify(expr: &Expr) -> Expr {
    let profile = if crate::profile::enabled() {
        expr.as_relation_aggregate().map(|(_, relation, _, _)| {
            (relation_profile_size(relation), Instant::now())
        })
    } else {
        None
    };
    let output = rewrite_count_zero_comparison(expr)
        .or_else(|| rewrite_empty_aggregate(expr))
        .or_else(|| rewrite_zero_sum(expr))
        .or_else(|| rewrite_index_zero_as_first(expr))
        .unwrap_or_else(|| expr.clone());
    if let Some((input_size, start)) = profile {
        let output_size = output.as_relation_aggregate()
            .map_or(0, |(_, relation, _, _)| relation_profile_size(relation));
        let skip_reason = (output == *expr).then_some("no_rewrite");
        crate::profile::rule_event(
            "math.relation.scope",
            input_size,
            output_size,
            start.elapsed().as_micros(),
            skip_reason,
        );
    }
    output
}

fn relation_profile_size(relation: &RelationExpr) -> usize {
    match relation {
        RelationExpr::Empty | RelationExpr::Base { .. } => 1,
        RelationExpr::Vector { value, .. } => 1 + value.size(),
        RelationExpr::Select { predicate: value, input }
        | RelationExpr::Project { value, input, .. } => 1 + value.size() + relation_profile_size(input),
        RelationExpr::Distinct(input) => 1 + relation_profile_size(input),
        RelationExpr::ProjectedUnion(columns) => 1 + columns.iter().len(),
        RelationExpr::Grouped(group) => 1 + group.columns().count(),
        RelationExpr::Join { on, filters, .. } =>
            1 + on.size() + filters.iter().map(Expr::size).sum::<usize>(),
    }
}

#[cfg(test)]
mod typed_count_bound_tests {
    use super::*;
    use crate::domains::relation::{relation_count, relation_count_nonnull, relation_first_value};
    use crate::BinOpKind;

    #[test]
    fn only_bare_count_is_evidence_for_a_relation_cardinality_bound() {
        let relation = RelationExpr::base("offers", "offer");
        let row = builder::row_value("offer");
        let zero = builder::lit_int(0);
        let count = relation_count(relation.clone());
        let comparison = builder::lt(zero.clone(), count);
        assert_eq!(relation_count_bound(&comparison), Some((BinOpKind::Gt, &relation, 0)));

        let first = relation_first_value(relation.clone(), row.clone(), builder::lit_int(5));
        let nonnull = relation_count_nonnull(relation, row);
        for value in [first, nonnull] {
            assert!(relation_count_bound(&builder::gt(value, zero.clone())).is_none());
        }
    }

    #[test]
    fn an_empty_inner_relation_contributes_zero_for_every_outer_row() {
        use crate::domains::relation::relation_sum;
        let outer = RelationExpr::base("accounts", "account");
        let inner = RelationExpr::select(builder::lit_bool(false), RelationExpr::base("bills", "bill"));
        let nested = relation_sum(outer.clone(), relation_sum(inner, builder::lit_int(1)));
        assert_eq!(crate::normalize(nested), builder::lit_int(0));
        let nonzero = relation_sum(outer, builder::lit_int(1));
        assert_ne!(crate::normalize(nonzero), builder::lit_int(0));
    }
}
