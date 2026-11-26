//! Source admission for waits whose only exit depends on an external response.
use simulator_parser::{
    JavaBinOp, JavaExpr, JavaLit, JavaProgram, JavaStatement, JavaType, JavaUnaryOp,
};
use std::collections::BTreeMap;

pub(super) fn validate_entry(program: &JavaProgram, entry: &str) -> Result<(), String> {
    let Some((class, method)) = entry
        .rsplit_once('.')
        .and_then(|(owner, name)| Some((program.find_class(owner)?, name)))
    else { return Ok(()); };
    let mut candidates = class.methods().iter().filter(|m| m.name() == method);
    let Some(method) = candidates.next().filter(|_| candidates.next().is_none()) else {
        return Ok(()); // Let normal entry admission resolve or diagnose the entry.
    };
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
                let boundary = |expr| external_call(program, &bindings, expr);
                let only_external_calls = || body.iter().all(|statement| {
                    matches!(statement, JavaStatement::ExprStmt(expr) if boundary(expr).is_some())
                });
                if let Some(probe) = poll_operand(cond).and_then(boundary).filter(|_| only_external_calls()) {
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
fn poll_operand(expr: &JavaExpr) -> Option<&JavaExpr> {
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
            _ => None,
        },
        _ => Some(expr),
    }
}

fn external_call(
    program: &JavaProgram,
    bindings: &BTreeMap<&str, &JavaType>,
    expr: &JavaExpr,
) -> Option<String> {
    let (receiver, method, args) = match expr {
        JavaExpr::MethodCall { target: Some(receiver), method, args }
            => (receiver, method, args),
        _ => return None,
    };
    let JavaExpr::Ident(receiver_name) = receiver.as_ref() else { return None; };
    let qualifies = args.iter().all(|arg| matches!(arg, JavaExpr::Ident(_) | JavaExpr::Lit(_)));
    let receiver_type = bindings.get(receiver_name.as_str()).filter(|_| qualifies)?;
    let qualified = match receiver_type {
        JavaType::Class(qualified) => qualified,
        _ => return None,
    };
    let interface = program.find_class(qualified).filter(|class| class.is_interface())?;
    (!program.classes().iter().any(|class| class.interfaces().iter()
        .any(|ty| ty == qualified || ty == interface.simple_name()))
        && super::source_ports::declares_bodyless_member(interface, method, args.len()))
        .then(|| format!("{qualified}.{method}"))
}
