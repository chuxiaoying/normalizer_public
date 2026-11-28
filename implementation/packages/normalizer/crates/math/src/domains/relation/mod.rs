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
pub mod positional;
pub use order_by::{order_by, order_components, order_value_components};

use crate::{
    Expr, builder,
    expr::{ExprKind, RelationAggregateKind},
};
use crate::{canonical_order_key, DefaultSortContext};

/// Physical table provenance, optionally refined to one acquired snapshot.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TableSource {
    pub name: String,
    pub snapshot: Option<u64>,
}

/// A projection preserves both its acquired source and the source's domain.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ProjectedColumn {
    pub table: TableSource,
    pub column: String,
    pub filters: Vec<Expr>,
}

/// Nonempty canonical set of projected source columns with their filters.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ProjectedColumns(Vec<ProjectedColumn>);

impl ProjectedColumns {
    pub fn iter(&self) -> impl Iterator<Item = &ProjectedColumn> + ExactSizeIterator {
        self.0.iter()
    }

    pub(crate) fn new(mut columns: Vec<ProjectedColumn>) -> Self {
        for branch in &mut columns {
            branch.filters.retain(|predicate| predicate.as_bool() != Some(true));
            branch.filters.sort_by_cached_key(|predicate| canonical_order_key(predicate, &DefaultSortContext));
            branch.filters.dedup();
        }
        columns.sort_by_cached_key(|branch| (
            branch.table.clone(), branch.column.clone(),
            branch.filters.iter().map(|predicate| canonical_order_key(predicate, &DefaultSortContext)).collect::<Vec<_>>(),
        ));
        columns.dedup();
        Self(columns)
    }
}

/// An ordered, nonempty tuple of columns bound to one physical table.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct GroupedColumns {
    table: TableSource,
    first: String,
    remaining: Vec<String>,
}

impl GroupedColumns {
    pub fn table(&self) -> &TableSource { &self.table }

    pub fn columns(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.first.as_str()).chain(self.remaining.iter().map(String::as_str))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum RelationExpr {
    Empty,
    Base {
        name: String,
        alias: String,
        snapshot: Option<u64>,
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
    ProjectedUnion(ProjectedColumns),
    Grouped(GroupedColumns),
    /// Eager projection of a validated integer field path from an ordered record array.
    RecordProjection { source: String, field: String },
    /// Inner join of two bound table aliases. Predicates remain separate
    /// so a generic nested SUM cannot masquerade as an authored join.
    Join {
        left: (TableSource, String),
        right: (TableSource, String),
        on: Expr,
        filters: Vec<Expr>,
    },
}

impl RelationExpr {
    pub fn base<S1: Into<String>, S2: Into<String>>(name: S1, alias: S2) -> Self {
        Self::Base {
            name: name.into(),
            alias: alias.into(),
            snapshot: None,
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

    pub fn projected_union(
        first: ProjectedColumn,
        remaining: impl IntoIterator<Item = ProjectedColumn>,
    ) -> Self {
        Self::ProjectedUnion(ProjectedColumns::new(std::iter::once(first).chain(remaining).collect()))
    }

    pub fn grouped(table: TableSource, first: String, remaining: Vec<String>) -> Self {
        Self::Grouped(GroupedColumns { table, first, remaining })
    }

    /// Visit the identities of concrete table reads carried by this relation.
    /// Child scalar expressions are visited separately by Expr's walker.
    pub fn visit_acquisitions(&self, visit: &mut impl FnMut(u64)) {
        match self {
            Self::Base { snapshot: Some(id), .. } => visit(*id),
            Self::Select { input, .. } | Self::Project { input, .. } | Self::Distinct(input) =>
                input.visit_acquisitions(visit),
            Self::ProjectedUnion(columns) => columns.iter().for_each(|branch| {
                if let Some(id) = branch.table.snapshot { visit(id); }
            }),
            Self::Grouped(group) => {
                if let Some(id) = group.table().snapshot { visit(id); }
            }
            Self::Join { left, right, .. } => {
                for (table, _) in [left, right] {
                    if let Some(id) = table.snapshot { visit(id); }
                }
            }
            _ => {}
        }
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
        RelationExpr::Vector { value, alias } => RelationExpr::Vector {
            value: crate::normalize(value),
            alias,
        },
        RelationExpr::Join { left, right, on, filters } => {
            let on = crate::normalize(on);
            let where_all = crate::normalize(filters.into_iter().fold(builder::lit_bool(true), builder::and));
            if crate::normalize(builder::and(on.clone(), where_all.clone())).as_bool() == Some(false) {
                return RelationExpr::Empty;
            }
            let mut filters = Vec::new();
            where_all.for_each_conjunct(|term| {
                if term.as_bool() != Some(true) { filters.push(term.clone()); }
            });
            RelationExpr::Join { left, right, on, filters }
        }
        leaf @ (RelationExpr::Empty | RelationExpr::Base { .. } | RelationExpr::ProjectedUnion(_) | RelationExpr::Grouped(_) | RelationExpr::RecordProjection { .. }) => leaf,
    }
}

/// Rebind an authored single-source predicate to one input of an inner
/// join. The snapshot stays fixed and only the lexical row slot changes.
/// No predicate may read through a different source or a nested binder.
pub fn rebind_join_predicate(
    value: &Expr, table: &TableSource, row_slot: usize, alias: &str,
) -> Option<Expr> {
    Some(match value.kind() {
        ExprKind::RowValue { sql: Some(column), .. } if &column.table == table && column.row_slot == 0 =>
            builder::sql_column(alias, table.clone(), row_slot, &column.column, column.scalar_type),
        ExprKind::Lit(_) | ExprKind::SymVar { .. } => value.clone(),
        ExprKind::BinOp { op, left, right } =>
            builder::binop_public(*op, rebind_join_predicate(left, table, row_slot, alias)?,
                rebind_join_predicate(right, table, row_slot, alias)?),
        _ => return None,
    })
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

/// Construct NthValue(index) over an ordered-list relation, with implicit
/// null for an absent row. An explicit default belongs to FirstValue.
///
/// `index` is an ordinary math Expr.  The relation must denote OrderedList
/// semantics at the executor/runtime boundary; math keeps this Java-free and
/// only provides the ordered-list query form.
pub fn relation_nth_value(relation: RelationExpr, index: Expr) -> Expr {
    Expr::new(ExprKind::RelationAggregate {
        kind: RelationAggregateKind::NthValue,
        relation: Box::new(relation),
        select_expr: Some(index),
        default_expr: None,
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
