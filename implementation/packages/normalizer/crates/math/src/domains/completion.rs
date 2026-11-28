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
        "completion.is_return" => "completion.is_return",
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
/// All positive thrown-class terms in a guard must belong to one receipt.
pub(crate) fn failure_partition(test: &Expr) -> Option<(&Expr, Vec<&str>)> {
    fn visit<'a>(test: &'a Expr, receipt: &mut Option<&'a Expr>, classes: &mut Vec<&'a str>) -> Option<()> {
        if let Some((crate::BinOpKind::Or, left, right)) = test.as_binop() {
            visit(left, receipt, classes)?;
            return visit(right, receipt, classes);
        }
        let (value, class) = throw_tag(test)?;
        if receipt.is_some_and(|earlier| earlier != value) { return None; }
        *receipt = Some(value);
        classes.push(class);
        Some(())
    }
    let mut receipt = None;
    let mut classes = Vec::new();
    visit(test, &mut receipt, &mut classes)?;
    classes.sort_unstable();
    classes.dedup();
    Some((receipt?, classes))
}

pub(crate) fn closed_return(receipt: Expr, classes: &[&str]) -> Expr {
    if let [class] = classes { return singleton_return(receipt, class); }
    let key = crate::register_func(
        "completion.is_return", vec![Primitive::Composite, Primitive::Composite],
        Some(Primitive::Boolean), FuncSemantics::pure_function(),
    );
    b::call(key, vec![receipt, b::array_lit(classes.iter().map(|class| b::lit_str(*class)).collect())])
}

pub fn multi_failure_return<'a>(name: &str, args: &'a [Expr]) -> Option<(&'a Expr, Vec<&'a str>)> {
    if name != "completion.is_return" { return None; }
    let [receipt, classes] = args else { return None; };
    let classes = classes.as_array_lit()?.iter().map(|class| match class.as_lit()? {
        crate::Lit::Str(name) => Some(name.as_str()),
        _ => None,
    }).collect::<Option<Vec<_>>>()?;
    Some((receipt, classes))
}

/// Caller must have proved a total acquired completion with exactly this
/// one failure class. The second slot carries that declaration, not a value read.
pub(crate) fn singleton_return(receipt: Expr, exception: &str) -> Expr {
    let key = crate::register_func(
        "completion.is_return",
        vec![Primitive::Composite, Primitive::String],
        Some(Primitive::Boolean),
        FuncSemantics::pure_function(),
    );
    b::call(key, vec![receipt, b::lit_str(exception)])
}

fn singleton_return_tag(test: &Expr) -> Option<(&Expr, &str)> {
    let (key, args) = test.as_call()?;
    let (operation, receipt, class) = class_observation(&crate::func_name(key), args)?;
    (operation == "completion.is_return").then_some((receipt, class))
}

pub(crate) fn single_failure_complement(a: &Expr, b: &Expr) -> bool {
    [(a, b), (b, a)].into_iter().any(|(success, failure)|
        singleton_return_tag(success).zip(throw_tag(failure))
            .is_some_and(|(a, b)| a == b))
}

pub(crate) fn failure_domain_of_typed_return(expr: &Expr) -> Option<Expr> {
    if let Some((receipt, class)) = singleton_return_tag(expr) {
        return Some(is_throw(receipt.clone(), class));
    }
    let (key, args) = expr.as_call()?;
    let (receipt, classes) = multi_failure_return(&crate::func_name(key), args)?;
    Some(classes.into_iter().fold(b::lit_bool(false), |guard, class|
        b::or(guard, is_throw(receipt.clone(), class))))
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
