//! An ordered relation produced by sorting the input on a sequence of
//! ascending non-null integer keys in lexical order. The source and each
//! key are expression children and therefore participate in substitution,
//! equality and hashing. Equal keys introduce no additional ordering.
use super::{relation_expr, RelationExpr};
use crate::{builder as b, Expr, Primitive};

pub fn order_by(input: RelationExpr, keys: Vec<Expr>) -> RelationExpr {
    if keys.is_empty() {
        return input;
    }
    let op = crate::register_func(
        &format!("relation.order_by/{}", keys.len()),
        std::iter::once(Primitive::Composite)
            .chain(keys.iter().map(|_| Primitive::Number))
            .collect(),
        Some(Primitive::Composite),
        crate::domains::primitive::registry::FuncSemantics::pure_function(),
    );
    let args = std::iter::once(relation_expr(input)).chain(keys).collect();
    RelationExpr::vector(b::call(op, args), "ordered")
}

pub fn order_components(relation: &RelationExpr) -> Option<(&RelationExpr, &[Expr])> {
    let RelationExpr::Vector { value, .. } = relation else {
        return None;
    };
    let (op, args) = value.as_call()?;
    let (source, keys) = args.split_first()?;
    if keys.is_empty() || crate::func_name(op) != format!("relation.order_by/{}", keys.len()) {
        return None;
    }
    Some((source.as_relation()?, keys))
}
