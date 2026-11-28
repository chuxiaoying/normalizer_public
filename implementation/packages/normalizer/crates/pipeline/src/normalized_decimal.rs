//! Binding and checked rules for standard exact decimal operations.
#[derive(Clone, Copy)]
pub(crate) enum DecimalIntrinsic {
    Multiply,
    Round,
}

pub(crate) fn decimal_intrinsic(
    program: &simulator_parser::JavaProgram,
    class: &simulator_parser::JavaClass,
    target: &str,
) -> Option<DecimalIntrinsic> {
    if !crate::normalized_text::bound_receiver(program, class, "N") {
        return None;
    }
    match target {
        "N.multiply" => Some(DecimalIntrinsic::Multiply),
        "N.round" => Some(DecimalIntrinsic::Round),
        _ => None,
    }
}

pub(crate) fn round_mode(
    mode: &simulator_parser::JavaExpr,
) -> Option<math::domains::decimal::Mode> {
    use math::domains::decimal::Mode;
    use simulator_parser::JavaExpr as E;
    let E::FieldAccess { target, field } = mode else {
        return None;
    };
    if !matches!(target.as_ref(), E::Ident(name) if name == "N") {
        return None;
    }
    match field.as_str() {
        "HALF_UP" => Some(Mode::HalfUp),
        "HALF_EVEN" => Some(Mode::HalfEven),
        _ => None,
    }
}
