//! Source admission for waits whose only exit depends on an external response.
use simulator_parser::{
    JavaBinOp, JavaClass, JavaExpr, JavaLit, JavaProgram, JavaStatement, JavaType, JavaUnaryOp,
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
    let mut bindings = BTreeMap::new();
    for field in class.fields() {
        bind_interface(program, &mut bindings, field.name(), field.java_type());
    }
    for (ty, name) in method.params() {
        bind_interface(program, &mut bindings, name, ty);
    }

    for statement in method.body().unwrap_or_default() {
        match statement {
            JavaStatement::VarDecl {
                name, java_type, ..
            } => {
                bind_interface(program, &mut bindings, name, java_type);
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
            (operand, JavaExpr::Lit(JavaLit::Bool(_))) => poll_operand(operand),
            _ => None,
        },
        _ => Some(expr),
    }
}

// Store only declared interfaces; a non-interface declaration still shadows
// an earlier field/parameter with the same name and clears its certificate.
type InterfaceBindings<'a> = BTreeMap<&'a str, (&'a str, &'a JavaClass)>;

fn bind_interface<'a>(
    program: &'a JavaProgram,
    bindings: &mut InterfaceBindings<'a>,
    name: &'a str,
    ty: &'a JavaType,
) {
    let interface = match ty {
        JavaType::Class(qualified) => program.find_class(qualified)
            .filter(|class| class.is_interface())
            .map(|class| (qualified.as_str(), class)),
        _ => None,
    };
    if let Some(interface) = interface {
        bindings.insert(name, interface);
    } else {
        bindings.remove(name);
    }
}

fn external_call(
    program: &JavaProgram,
    bindings: &InterfaceBindings<'_>,
    expr: &JavaExpr,
) -> Option<String> {
    let (receiver, method, args) = match expr {
        JavaExpr::MethodCall { target: Some(receiver), method, args }
            if args.iter().all(|arg| matches!(arg, JavaExpr::Ident(_) | JavaExpr::Lit(_)))
            => (receiver.as_ref(), method, args),
        _ => return None,
    };
    let JavaExpr::Ident(receiver) = receiver else { return None; };
    let &(qualified, interface) = bindings.get(receiver.as_str())?;
    (!program.classes().iter().any(|class| class.interfaces().iter()
        .any(|ty| ty.as_str() == qualified))
        && super::source_ports::declares_bodyless_member(interface, method, args.len()))
        .then(|| format!("{qualified}.{method}"))
}
