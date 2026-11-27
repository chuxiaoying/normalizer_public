//! rebuild-with-children 骨架 —— 递归化简子节点后重建同型节点。

use crate::expr::{Expr, ExprKind};

/// 递归化简所有子表达式，用原节点的身份/运算符/FuncKey 原样重建。
pub(crate) fn rebuild_with_children_simplified(
    expr: &Expr,
    recurse: &dyn Fn(&Expr) -> Expr,
) -> Expr {
    match expr.kind() {
        // 叶子节点：无子表达式，原样返回。
        // HeapRef 也是叶子（不参与代数化简，passthrough）。
        // Pending 也是叶子（未知动态类型值的符号占位，不参与化简）。
        ExprKind::Lit(_)
        | ExprKind::SymVar { .. }
        | ExprKind::HeapRef(_)
        | ExprKind::RowValue { .. } => expr.clone(),

        ExprKind::RelationAggregate {
            kind,
            relation,
            select_expr,
            default_expr,
        } => {
            let rebuilt_relation = rebuild_relation_with_children_simplified(relation, recurse);
            let new_relation = crate::domains::relation::normalize_relation(rebuilt_relation);
            let new_select = select_expr.as_ref().map(recurse);
            let new_default = default_expr.as_ref().map(recurse);
            if &new_relation == relation.as_ref()
                && new_select.as_ref() == select_expr.as_ref()
                && new_default.as_ref() == default_expr.as_ref()
            {
                expr.clone()
            } else {
                Expr::new(ExprKind::RelationAggregate {
                    kind: *kind,
                    relation: Box::new(new_relation),
                    select_expr: new_select,
                    default_expr: new_default,
                })
            }
        }

        ExprKind::Relation(relation) => {
            let rebuilt_relation = rebuild_relation_with_children_simplified(relation, recurse);
            let new_relation = crate::domains::relation::normalize_relation(rebuilt_relation);
            if &new_relation == relation.as_ref() {
                expr.clone()
            } else {
                Expr::new(ExprKind::Relation(Box::new(new_relation)))
            }
        }

        ExprKind::Call {
            key,
            args,
        } => {
            let new_args: Vec<Expr> = args.iter().map(|a| recurse(a)).collect();
            if new_args == *args {
                expr.clone()
            } else {
                Expr::new(ExprKind::Call {
                    key: *key,
                    args: new_args,
                })
            }
        }

        ExprKind::BinOp { op, left, right } => {
            let nl = recurse(left);
            let nr = recurse(right);
            if &nl == left && &nr == right {
                expr.clone()
            } else {
                Expr::new(ExprKind::BinOp {
                    op: *op,
                    left: nl,
                    right: nr,
                })
            }
        }

        ExprKind::UnaryOp { op, operand } => {
            let no = recurse(operand);
            if &no == operand {
                expr.clone()
            } else {
                Expr::new(ExprKind::UnaryOp {
                    op: *op,
                    operand: no,
                })
            }
        }

        ExprKind::CaseWhen { branches } => {
            let new_branches: Vec<(Expr, Expr)> = branches
                .iter()
                .map(|(c, v)| (recurse(c), recurse(v)))
                .collect();
            if new_branches == *branches {
                expr.clone()
            } else {
                Expr::new(ExprKind::CaseWhen {
                    branches: new_branches,
                })
            }
        }

        ExprKind::ArrayLit(_) => expr.clone(),
    }
}

fn rebuild_relation_with_children_simplified(
    relation: &crate::domains::relation::RelationExpr,
    recurse: &dyn Fn(&Expr) -> Expr,
) -> crate::domains::relation::RelationExpr {
    use crate::domains::relation::RelationExpr;

    match relation {
        RelationExpr::Empty => RelationExpr::Empty,
        RelationExpr::Base { name, alias } => RelationExpr::Base {
            name: name.clone(),
            alias: alias.clone(),
        },
        RelationExpr::Vector { value, alias } => RelationExpr::Vector {
            value: recurse(value),
            alias: alias.clone(),
        },
        RelationExpr::Select { predicate, input } => RelationExpr::select(
            recurse(predicate),
            rebuild_relation_with_children_simplified(input, recurse),
        ),
        RelationExpr::Project {
            value,
            alias,
            input,
        } => RelationExpr::Project {
            value: recurse(value),
            alias: alias.clone(),
            input: Box::new(rebuild_relation_with_children_simplified(input, recurse)),
        },
        RelationExpr::Distinct(input) => RelationExpr::Distinct(Box::new(
            rebuild_relation_with_children_simplified(input, recurse),
        )),
        RelationExpr::ProjectedUnion(_) => relation.clone(),
    }
}

#[cfg(test)]
mod aggregate_rebuild_tests {
    use super::*;
    use crate::builder as b;
    use crate::domains::relation::{relation_first_value, RelationExpr};

    #[test]
    fn first_child_rebuild_preserves_source_projection_and_default() {
        let rows = RelationExpr::base("offers", "offer");
        let field = b::row_value("offer");
        let fallback = b::add(b::lit_int(2), b::lit_int(3));
        let expression = relation_first_value(rows.clone(), field.clone(), fallback);

        let rebuilt = rebuild_with_children_simplified(&expression, &|child| crate::normalize(child.clone()));
        assert_eq!(rebuilt, relation_first_value(rows, field, b::lit_int(5)));
        assert_eq!(
            rebuild_with_children_simplified(&rebuilt, &|child| child.clone()),
            rebuilt
        );
    }
}
