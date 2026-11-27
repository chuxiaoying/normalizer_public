//! Closed disjoint completion values: Return(value) | Throw(class, payload).
//!
//! A boundary's declared variants are exhaustive and mutually exclusive.
//! is_throw is total. value/payload are partial projections, defined only in
//! the corresponding variant; callers must retain the appropriate domain.
use crate::{builder as b, Expr, FuncSemantics, Primitive};

pub fn output_name(name: &str) -> &str {
    match name.split_once('@') {
        Some((base, "number" | "boolean" | "string" | "identity" | "composite"))
            if matches!(
                base,
                "completion.value" | "completion.payload"
            ) =>
        {
            base
        }
        _ => name,
    }
}
/// The class-bearing completion operations share the same checked operand shape.
pub fn class_observation<'a>(registered_name: &str, args: &'a [Expr]) -> Option<(&'static str, &'a Expr, &'a str)> {
    let operation = match output_name(registered_name) {
        "completion.is_throw" => "completion.is_throw",
        "completion.payload" => "completion.payload",
        _ => return None,
    };
    let [receipt, class] = args else { return None; };
    let crate::Lit::Str(class) = class.as_lit()? else { return None; };
    Some((operation, receipt, class))
}

/// A positive throw test on a typed completion, not on the port, its
/// payload or a general Boolean value.
pub fn throw_tag(test: &Expr) -> Option<(&Expr, &str)> {
    let (key, args) = test.as_call()?;
    let (operation, receipt, class) = class_observation(&crate::func_name(key), args)?;
    if operation != "completion.is_throw"
        || !matches!(receipt.as_sym_var(), Some(crate::SymVarOrigin::External { path, primitive_type, .. }) if path.is_empty() && primitive_type == "normalized.completion")
    {
        return None;
    }
    Some((receipt, class))
}

/// Different class tags of one acquired completion cannot both match. A
/// separate receipt is a separate event, even for the same port and args.
pub(crate) fn different_throw_tags(left: &Expr, right: &Expr) -> bool {
    let (Some((left_receipt, left_class)), Some((right_receipt, right_class))) =
        (throw_tag(left), throw_tag(right))
    else { return false; };
    left_receipt == right_receipt && left_class != right_class
}

/// On a terminal guard, keep a later receipt's test under the tag that
/// guards its occurrence. A different tag of that same earlier receipt can
/// be checked in the fallback without inspecting the later receipt.
pub(crate) fn factor_terminal_tag_guard(value: &Expr) -> Option<Expr> {
    use crate::BinOpKind::{And, Or};
    let (Or, left, right) = value.as_binop()? else { return None; };
    for (earlier, guarded) in [(left, right), (right, left)] {
        let Some((And, a, z)) = guarded.as_binop() else { continue; };
        for (tag, rest) in [(a, z), (z, a)] {
            if different_throw_tags(earlier, tag) {
                return Some(b::case_when(vec![(tag.clone(), rest.clone())], earlier.clone()));
            }
        }
    }
    None
}

fn typed(
    name: &str,
    args: Vec<Expr>,
    params: Vec<Primitive>,
    result: Primitive,
) -> Expr {
    let key = crate::register_func(
        &format!("{name}@{}", result.algebra_name()),
        params,
        Some(result),
        FuncSemantics::pure_function(),
    );
    crate::normalize(b::call(key, args))
}
pub fn is_throw(value: Expr, class: &str) -> Expr {
    let key = crate::register_func(
        "completion.is_throw",
        vec![Primitive::Composite, Primitive::String],
        Some(Primitive::Boolean),
        FuncSemantics::pure_function(),
    );
    crate::normalize(b::call(key, vec![value, b::lit_str(class)]))
}
pub fn value(completion: Expr, ty: Primitive) -> Expr {
    typed(
        "completion.value",
        vec![completion],
        vec![Primitive::Composite],
        ty,
    )
}
pub fn payload(completion: Expr, class: &str, ty: Primitive) -> Expr {
    typed(
        "completion.payload",
        vec![completion, b::lit_str(class)],
        vec![Primitive::Composite, Primitive::String],
        ty,
    )
}

#[cfg(test)]
mod tag_partitions {
    use super::*;

    #[test]
    fn terminal_guard_keeps_the_second_observation_under_its_issue_tag() {
        let first = b::external_completion(0);
        let second = b::external_completion(1);
        let retry = is_throw(first.clone(), "Transient");
        let refused = is_throw(first, "Terminal");
        let second_terminal = is_throw(second.clone(), "Terminal");
        let ordinary = b::or(refused.clone(), b::and(retry.clone(), second_terminal.clone()));
        assert_eq!(factor_terminal_tag_guard(&ordinary),
            Some(b::case_when(vec![(retry, second_terminal.clone())], refused.clone())));
        let separate = b::or(refused, b::and(is_throw(second, "Transient"), second_terminal));
        assert!(factor_terminal_tag_guard(&separate).is_none());
    }

    #[test]
    fn one_present_receipt_has_only_one_throw_class() {
        let receipt = b::external_completion(0);
        let transient = is_throw(receipt.clone(), "Transient");
        let terminal = is_throw(receipt, "Terminal");
        let other = is_throw(b::external_completion(1), "Terminal");
        assert_eq!(crate::normalize(b::and(transient.clone(), terminal.clone())).as_bool(), Some(false));
        assert_eq!(crate::normalize(b::and(b::not(terminal.clone()), transient.clone())), transient);
        assert_eq!(crate::normalize(b::and(transient.clone(), b::not(terminal.clone()))), transient);
        assert!(b::path_implies(&transient, &b::not(terminal)));
        assert!(!b::path_implies(&transient, &b::not(other.clone())));
        assert!(crate::normalize(b::and(transient, other)).as_bool().is_none());
    }
}
