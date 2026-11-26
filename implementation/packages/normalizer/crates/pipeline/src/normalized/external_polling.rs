//! Source admission for waits whose only exit depends on an external response.
use simulator_parser::{
    JavaBinOp, JavaClass, JavaExpr, JavaLit, JavaProgram, JavaStatement, JavaType, JavaUnaryOp,
};
use std::collections::BTreeMap;

pub(super) fn validate_entry(program: &JavaProgram, entry: &str) -> Result<(), String> {
    let Some((owner, method)) = entry.rsplit_once('.') else {
        return Ok(());
    };
    let Some(class) = program.find_class(owner) else {
        return Ok(());
    };
    let mut candidates = class.methods().iter().filter(|m| m.name() == method);
    let Some(method) = candidates.next() else {
        return Ok(());
    };
    if candidates.next().is_some() {
        return Ok(()); // Let normal overload admission resolve or diagnose the entry.
    }
    let mut bindings: BTreeMap<_, _> = class
        .fields()
        .iter()
        .map(|f| (f.name(), f.java_type()))
        .collect();
    bindings.extend(method.params().iter().map(|(ty, name)| (name.as_str(), ty)));

    for statement in method.body().unwrap_or_default() {
        match statement {
            JavaStatement::VarDecl {
                name, java_type, ..
            } => {
                bindings.insert(name, java_type);
            }
            JavaStatement::Return(_) | JavaStatement::Throw(_) => break,
            JavaStatement::While { cond, body }
            | JavaStatement::For {
                init: None,
                cond: Some(cond),
                update: None,
                body,
            } => {
                let boundary = |expr| external_call(program, class, &bindings, expr);
                let Some(probe) = boundary(poll_operand(cond)) else {
                    continue;
                };
                // A local bound, an early exit, or an implementation in this
                // program needs ordinary control-flow analysis. Opaque calls
                // alone supply no guarantee that the next probe will change.
                let only_external_calls = body.iter().all(|statement| match statement {
                    JavaStatement::ExprStmt(expr) => boundary(expr).is_some(),
                    _ => false,
                });
                if only_external_calls {
                    return Err(crate::preprocess::required(
                        "NORM-EXTERNAL-POLL",
                        entry,
                        &format!("termination depends on external response from {probe}; no finite bound is declared"),
                        "Supply an explicit completion contract or a bounded polling policy.",
                    ));
                }
            }
            _ => {}
        }
    }
    Ok(())
}

// Negation and comparisons to boolean literals change which response exits
// the loop, but cannot establish when such a response will first occur.
fn poll_operand(expr: &JavaExpr) -> &JavaExpr {
    match expr {
        JavaExpr::UnaryOp {
            op: JavaUnaryOp::Not,
            operand,
        } => poll_operand(operand),
        JavaExpr::BinOp {
            op: JavaBinOp::Eq | JavaBinOp::Ne,
            left,
            right,
        } => match (left.as_ref(), right.as_ref()) {
            (operand, JavaExpr::Lit(JavaLit::Bool(_)))
            | (JavaExpr::Lit(JavaLit::Bool(_)), operand) => poll_operand(operand),
            _ => expr,
        },
        _ => expr,
    }
}

fn external_call(
    program: &JavaProgram,
    owner: &JavaClass,
    bindings: &BTreeMap<&str, &JavaType>,
    expr: &JavaExpr,
) -> Option<String> {
    let JavaExpr::MethodCall {
        target: Some(receiver),
        method,
        args,
    } = expr
    else {
        return None;
    };
    // A computed argument can itself transfer control or advance local state.
    if !args
        .iter()
        .all(|arg| matches!(arg, JavaExpr::Ident(_) | JavaExpr::Lit(_)))
    {
        return None;
    }
    // Keep lexical binding and explicit field lookup distinct while
    // sharing the admitted receiver syntax with other port calls.
    let receiver_name = super::source_ports::receiver(receiver)?;
    let receiver_type = if matches!(receiver.as_ref(), JavaExpr::Ident(_)) {
        *bindings.get(receiver_name)?
    } else {
        owner.fields().iter().find(|f| f.name() == receiver_name)?.java_type()
    };
    let JavaType::Class(qualified) = receiver_type else {
        return None;
    };
    let interface = program.find_class(qualified)?;
    if !interface.is_interface()
        || program.classes().iter().any(|class| {
            class
                .interfaces()
                .iter()
                .any(|ty| ty == qualified || ty == interface.simple_name())
        })
    {
        return None;
    }
    super::source_ports::declares_bodyless_member(interface, method, args.len())
        .then(|| format!("{qualified}.{method}"))
}
