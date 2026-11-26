//! An ordered relation produced by sorting the input on a sequence of
//! ascending non-null integer keys in lexical order. The source and each
//! key are expression children and therefore participate in substitution,
//! equality and hashing. Equal keys introduce no additional ordering.
use super::{relation_expr, RelationExpr};
use crate::{builder as b, Expr, Primitive};

/// Build an ordered relation from a certified first key and its ordered tail.
pub fn order_by(
    input: RelationExpr,
    first: Expr,
    remaining: impl ExactSizeIterator<Item = Expr>,
) -> RelationExpr {
    let key_count = 1 + remaining.len();
    let op = crate::register_func(
        &format!("relation.order_by/{key_count}"),
        std::iter::once(Primitive::Composite)
            .chain((0..key_count).map(|_| Primitive::Number))
            .collect(),
        Some(Primitive::Composite),
        crate::domains::primitive::registry::FuncSemantics::pure_function(),
    );
    let args = std::iter::once(relation_expr(input)).chain([first]).chain(remaining).collect();
    RelationExpr::vector(b::call(op, args), "ordered")
}

pub fn order_components(relation: &RelationExpr) -> Option<(&RelationExpr, &[Expr])> {
    let RelationExpr::Vector { value, .. } = relation else {
        return None;
    };
    order_value_components(value)
}

/// Recognize the ordered value carried by the relation and its child walk.
pub fn order_value_components(value: &Expr) -> Option<(&RelationExpr, &[Expr])> {
    let (op, args) = value.as_call()?;
    let (source, keys) = args.split_first()?;
    if keys.is_empty() || crate::func_name(op) != format!("relation.order_by/{}", keys.len()) {
        return None;
    }
    Some((source.as_relation()?, keys))
}

#[cfg(test)]
mod ordered_source_shape_tests {
    use super::*;
    use crate::domains::primitive::registry::FuncSemantics;

    #[test]
    fn composite_vector_call_shape_is_not_itself_an_ordering_certificate() {
        let rows = RelationExpr::base("rows", "row");
        let first = b::acquired_integer_row("row");
        let second = b::lit_int(3);
        let ordered = order_by(rows.clone(), first.clone(), [second.clone()].into_iter());
        let (source, keys) = order_components(&ordered).expect("order_by result");
        assert_eq!(source, &rows);
        assert_eq!(keys, &[first.clone(), second]);

        let generic = crate::register_func(
            "test.relation.opaque_selection",
            vec![Primitive::Composite, Primitive::Number],
            Some(Primitive::Composite),
            FuncSemantics::pure_function(),
        );
        let value = b::call(generic, vec![relation_expr(rows), first]);
        let opaque = RelationExpr::vector(value, "opaque");
        assert!(order_components(&opaque).is_none());
    }
}
