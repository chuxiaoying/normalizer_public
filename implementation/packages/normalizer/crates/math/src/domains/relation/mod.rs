//! Relation Calculus domain.
//!
//! This module is intentionally Java-free.  Relation Calculus is not a second scalar/boolean
//! IR: there is only one scalar expression IR in math, [`crate::Expr`].
//! Relation algebra is modelled here as relation-valued structure whose
//! predicates, projections, event payloads, and query results use the existing
//! `Expr` API.
//!
//! Current boundary:
//! - `RelationExpr` is still a relation-valued AST local to this module;
//! - scalar/boolean/count results are existing `Expr` values;
//! - no `BoolExpr`, `CountExpr`, `Predicate`, or scalar `Value` parallel world
//!   is admitted here.

mod order_by;
pub use order_by::{order_by, order_components};

use crate::{
    Expr, builder,
    expr::{ExprKind, RelationAggregateKind},
};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum RelationExpr {
    Empty,
    Base {
        name: String,
        alias: String,
    },
    /// A finite ordered relation backed by an immutable vector expression.
    ///
    /// This keeps a materialized vector in the same relation algebra as a
    /// source sequence. Its element expression is evaluated once as the
    /// vector value, while `alias` names the current row item for predicates
    /// and projections.
    Vector {
        value: Expr,
        alias: String,
    },
    Select {
        predicate: Expr,
        input: Box<RelationExpr>,
    },
    Project {
        value: Expr,
        alias: String,
        input: Box<RelationExpr>,
    },
    Distinct(Box<RelationExpr>),
    /// Unordered set union of already projected branches.
    Union(Vec<RelationExpr>),
}

impl RelationExpr {
    pub fn base<S1: Into<String>, S2: Into<String>>(name: S1, alias: S2) -> Self {
        Self::Base {
            name: name.into(),
            alias: alias.into(),
        }
    }

    pub fn vector<S: Into<String>>(value: Expr, alias: S) -> Self {
        Self::Vector {
            value,
            alias: alias.into(),
        }
    }

    pub fn select(predicate: Expr, input: RelationExpr) -> Self {
        if predicate.as_bool() == Some(true) {
            return input;
        }
        Self::Select {
            predicate,
            input: Box::new(input),
        }
    }

    pub fn project<S: Into<String>>(value: Expr, alias: S, input: RelationExpr) -> Self {
        Self::Project {
            value,
            alias: alias.into(),
            input: Box::new(input),
        }
    }

    pub fn union(branches: Vec<RelationExpr>) -> Self {
        normalize_relation(Self::Union(branches))
    }

    pub fn distinct(input: RelationExpr) -> Self {
        Self::Distinct(Box::new(input))
    }
}

fn rewrite_relation(expr: &RelationExpr) -> Option<RelationExpr> {
    match expr {
        RelationExpr::Select { predicate, input } => {
            // Both predicates are pure and refer to the same row binding.
            if let RelationExpr::Select {
                predicate: inner,
                input: source,
            } = input.as_ref()
            {
                return Some(RelationExpr::Select {
                    predicate: builder::and(inner.clone(), predicate.clone()),
                    input: source.clone(),
                });
            }
            match (predicate.as_bool(), input.as_ref()) {
                (Some(true), source) => Some(source.clone()),
                (Some(false), _) | (_, RelationExpr::Empty) => Some(RelationExpr::Empty),
                _ => None,
            }
        }
        // Sorting changes only the order of extant rows. Its typed
        // key expressions are not evaluated for an empty row domain.
        RelationExpr::Vector { .. }
            if order_components(expr)
                .is_some_and(|(source, _)| matches!(source, RelationExpr::Empty)) =>
        {
            Some(RelationExpr::Empty)
        }
        // Duplicate elimination cannot add a row.
        RelationExpr::Distinct(input) if matches!(input.as_ref(), RelationExpr::Empty) => {
            Some(RelationExpr::Empty)
        }
        _ => None,
    }
}

pub fn normalize_relation(expr: RelationExpr) -> RelationExpr {
    let mut current = normalize_relation_children(expr);
    while let Some(next) = rewrite_relation(&current) {
        current = normalize_relation_children(next);
    }
    current
}

fn normalize_relation_children(expr: RelationExpr) -> RelationExpr {
    match expr {
        RelationExpr::Select { predicate, input } => RelationExpr::Select {
            predicate: crate::normalize(predicate),
            input: Box::new(normalize_relation(*input)),
        },
        RelationExpr::Project {
            value,
            alias,
            input,
        } => RelationExpr::Project {
            value,
            alias,
            input: Box::new(normalize_relation(*input)),
        },
        RelationExpr::Distinct(input) => {
            RelationExpr::Distinct(Box::new(normalize_relation(*input)))
        }
        RelationExpr::Union(branches) => {
            let mut branches: Vec<_> = branches.into_iter().map(normalize_relation).collect();
            branches.sort_by_cached_key(|branch| {
                let structural = crate::canonical_order_key(&relation_expr(branch.clone()), &crate::DefaultSortContext);
                let projected = match branch {
                    RelationExpr::Project {value, ..} => crate::canonical_order_key(value, &crate::DefaultSortContext),
                    _ => structural.clone(),
                };
                (projected, structural)
            });
            branches.dedup();
            RelationExpr::Union(branches)
        }
        RelationExpr::Vector { value, alias } => RelationExpr::Vector {
            value: crate::normalize(value),
            alias,
        },
        leaf @ (RelationExpr::Empty | RelationExpr::Base { .. }) => leaf,
    }
}

/// Wrap a relation-valued Relation Calculus expression into the single math Expr IR.
pub fn relation_expr(relation: RelationExpr) -> Expr {
    Expr::new(ExprKind::Relation(Box::new(relation)))
}

/// Construct Exists(relation) as a structured aggregate inside the existing
/// math `Expr` IR.
///
/// The `select_expr = true` payload is part of the aggregate representation; it
/// is not a resurrected BoolExpr side language.
pub fn relation_exists(relation: RelationExpr) -> Expr {
    Expr::new(ExprKind::RelationAggregate {
        kind: RelationAggregateKind::Exists,
        relation: Box::new(relation),
        select_expr: Some(builder::lit_bool(true)),
        default_expr: None,
    })
}

/// Construct Count(relation) as a structured aggregate inside the existing
/// math `Expr` IR.
pub fn relation_count(relation: RelationExpr) -> Expr {
    Expr::new(ExprKind::RelationAggregate {
        kind: RelationAggregateKind::Count,
        relation: Box::new(relation),
        select_expr: None,
        default_expr: None,
    })
}

/// Count the values of a total non-null projection; callers certify this
/// domain rather than assuming SQL COUNT(expr) equals cardinality on nulls.
pub fn relation_count_nonnull(relation: RelationExpr, value: Expr) -> Expr {
    Expr::new(ExprKind::RelationAggregate {
        kind: RelationAggregateKind::Count,
        relation: Box::new(relation),
        select_expr: Some(value),
        default_expr: None,
    })
}

/// Construct projected Count(Distinct(...)) as a structured aggregate.
///
/// `distinct_expr` is an ordinary math `Expr` projection.  This helper denotes
/// projected distinct count, rather than distinctness of complete rows.
pub fn relation_count_distinct(relation: RelationExpr, distinct_expr: Expr) -> Expr {
    Expr::new(ExprKind::RelationAggregate {
        kind: RelationAggregateKind::Count,
        relation: Box::new(RelationExpr::distinct(relation)),
        select_expr: Some(distinct_expr),
        default_expr: None,
    })
}

/// Construct FirstValue(value) over a structured relation.
///
/// The selected value and default are ordinary math Expr nodes;
/// no string SQL side language is introduced here.
pub fn relation_first_value(relation: RelationExpr, value: Expr, default: Expr) -> Expr {
    Expr::new(ExprKind::RelationAggregate {
        kind: RelationAggregateKind::FirstValue,
        relation: Box::new(relation),
        select_expr: Some(value),
        default_expr: Some(default),
    })
}

/// Construct NthValue(index) over an ordered-list relation.
///
/// `index` is an ordinary math Expr.  The relation must denote OrderedList
/// semantics at the executor/runtime boundary; math keeps this Java-free and
/// only provides the ordered-list query form.
pub fn relation_nth_value(relation: RelationExpr, index: Expr, default: Expr) -> Expr {
    Expr::new(ExprKind::RelationAggregate {
        kind: RelationAggregateKind::NthValue,
        relation: Box::new(relation),
        select_expr: Some(index),
        default_expr: Some(default),
    })
}

/// Construct Sum(value) over a structured relation.
///
/// This is the Relation Calculus query form used when row-scope loop execution proves that
/// each matching row contributes a scalar value to an accumulator.  The selected
/// value is an ordinary math Expr over the relation row; no Java loop-shape
/// pattern language is introduced here.
pub fn relation_sum(relation: RelationExpr, value: Expr) -> Expr {
    Expr::new(ExprKind::RelationAggregate {
        kind: RelationAggregateKind::Sum,
        relation: Box::new(relation),
        select_expr: Some(value),
        default_expr: None,
    })
}
