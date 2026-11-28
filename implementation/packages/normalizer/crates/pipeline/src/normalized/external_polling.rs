//! Source admission for waits whose only exit depends on an external response.
use super::source_ports::SourcePort;
use simulator_parser::{JavaBinOp, JavaExpr, JavaLit, JavaProgram, JavaStatement, JavaUnaryOp};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn validate_entry(
    program: &JavaProgram,
    entry: &str,
    locator: &str,
    ports: &BTreeMap<String, SourcePort>,
) -> Result<(), String> {
    let Some((class, method)) = entry.rsplit_once('.').and_then(|(owner, _)| {
        let class = program.find_class(owner)?;
        Some((class, super::method_for_key(class, entry)?))
    }) else {
        return Ok(());
    }; // Let admission diagnose an unresolved entry.
    if method.is_static() {
        return Ok(());
    }
    let owner = class.full_name();
    let mut shadowed: BTreeSet<&str> = method
        .params()
        .iter()
        .map(|(_, name)| name.as_str())
        .collect();

    for statement in method.body().unwrap_or_default() {
        match statement {
            JavaStatement::VarDecl { name, .. } => {
                shadowed.insert(name.as_str());
            }
            JavaStatement::Return(_) | JavaStatement::Throw(_) => break,
            JavaStatement::While { cond, body }
            | JavaStatement::For {
                init: None,
                cond: Some(cond),
                update: None,
                body,
            } => {
                let boundary = |expr| external_call(&owner, ports, &shadowed, expr);
                let only_external_calls = || {
                    body.iter().all(|statement| {
                    matches!(statement, JavaStatement::ExprStmt(expr) if boundary(expr).is_some())
                })
                };
                if let Some(probe) = poll_operand(cond)
                    .and_then(boundary)
                    .filter(|_| only_external_calls())
                {
                    return Err(crate::preprocess::required(
                        "NORM-EXTERNAL-POLL",
                        locator,
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
    owner: &str,
    ports: &BTreeMap<String, SourcePort>,
    shadowed: &BTreeSet<&str>,
    expr: &JavaExpr,
) -> Option<String> {
    let (receiver, method, args) = match expr {
        JavaExpr::MethodCall {
            target: Some(receiver),
            method,
            args,
        } if args
            .iter()
            .all(|arg| matches!(arg, JavaExpr::Ident(_) | JavaExpr::Lit(_))) =>
        {
            (receiver.as_ref(), method, args)
        }
        _ => return None,
    };
    let field = match receiver {
        JavaExpr::Ident(name) if !shadowed.contains(name.as_str()) => name,
        JavaExpr::FieldAccess { target, field } if matches!(target.as_ref(), JavaExpr::Ident(name) if name == "this") => {
            field
        }
        _ => return None,
    };
    let port = ports.get(&format!("{owner}.{field}.{method}"))?;
    (port.params.len() == args.len()).then(|| format!("{}.{method}", port.interface))
}
