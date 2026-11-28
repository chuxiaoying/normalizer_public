//! Source linking for non-escaping local scalar SAMs. No closure value is
//! published. The certified callee gets ordinary Java argument evaluation
//! and completion; defining the lambda does not run that callee.
use crate::ast::*;
use std::collections::{BTreeMap, BTreeSet};

type Names = BTreeSet<String>;
type Callables = BTreeMap<String, Callable>;

#[derive(Clone)]
struct Callable {
    member: String,
    helper: String,
    arity: usize,
    captured: Vec<String>,
}

fn error(message: &str) -> String { format!("[JAVA-LOCAL-LAMBDA] {message}") }

fn scalar(ty: &JavaType) -> bool {
    matches!(ty, JavaType::Primitive(name) if matches!(name.as_str(), "int" | "long" | "boolean"))
}

pub(crate) fn link(program: &mut JavaProgram) -> Result<(), String> {
    let interfaces: BTreeMap<_, _> = program.classes.iter().filter_map(|class| {
        if !class.is_interface || class.supertype.is_some() || !class.interfaces.is_empty()
            || !class.type_parameters.is_empty() || !class.fields.is_empty() { return None; }
        let [sam] = class.methods.as_slice() else { return None; };
        (!sam.is_static && !sam.is_private && !sam.is_native && !sam.is_constructor
            && sam.body.is_none() && sam.type_parameters.is_empty() && sam.throws.is_empty()
            && scalar(&sam.return_type) && sam.params.iter().all(|(ty, _)| scalar(ty)))
            .then(|| (class.full_name(), sam.clone()))
    }).collect();
    for class in &mut program.classes {
        let mut helpers = Vec::new();
        for method in &mut class.methods {
            if let Some(body) = &mut method.body {
                lower(body, &method.params, &interfaces, &mut helpers)?;
            }
        }
        class.methods.extend(helpers);
    }
    Ok(())
}

fn lower(
    body: &mut Vec<JavaStatement>, parameters: &[(JavaType, String)],
    interfaces: &BTreeMap<String, JavaMethod>, helpers: &mut Vec<JavaMethod>,
) -> Result<(), String> {
    // Check target rebinding and effective finality across the entire
    // lexical method, including the branches that do not run on a path.
    let mut writes = Names::new();
    let mut declarations = BTreeMap::<String, usize>::new();
    let mut found = false;
    walk(body, &mut |expr| {
        match expr {
            JavaExpr::LocalLambda { .. } => found = true,
            JavaExpr::Assign { target, .. }
                | JavaExpr::UnaryOp { op: JavaUnaryOp::PreInc | JavaUnaryOp::PreDec
                    | JavaUnaryOp::PostInc | JavaUnaryOp::PostDec, operand: target } => {
                if let JavaExpr::Ident(name) = target.as_ref() { writes.insert(name.clone()); }
            }
            JavaExpr::InstanceOf { binding: Some(name), .. } =>
                *declarations.entry(name.clone()).or_default() += 1,
            _ => {}
        }
        Ok(())
    }, &mut |_| {})?;
    if !found { return Ok(()); }
    walk(body, &mut |_| Ok(()), &mut |name| *declarations.entry(name.to_owned()).or_default() += 1)?;
    for (_, name) in parameters { *declarations.entry(name.clone()).or_default() += 1; }
    let mut known = parameters.iter().map(|(ty, name)| (name.clone(), ty.clone())).collect::<BTreeMap<_, _>>();
    let mut local_names = parameters.iter().map(|(_, name)| name.clone()).collect::<Names>();
    let mut callables = Callables::new();
    let mut lowered = Vec::with_capacity(body.len());
    for mut stmt in std::mem::take(body) {
        let lambda = match &stmt {
            JavaStatement::VarDecl { java_type, name, initializer: Some(JavaExpr::LocalLambda { parameters, value }), .. } =>
                Some((java_type, name, parameters, value.as_ref())),
            _ => None,
        };
        if let Some((ty, name, lambda_params, value)) = lambda {
            if declarations.get(name) != Some(&1) || writes.contains(name) {
                return Err(error("local closure may not be shadowed or rebound"));
            }
            let sam = match ty { JavaType::Class(owner) => interfaces.get(owner), _ => None }
                .ok_or_else(|| error("local closure target must be a source-declared scalar SAM"))?;
            let formals: Names = lambda_params.iter().cloned().collect();
            if formals.len() != lambda_params.len() || lambda_params.len() != sam.params.len()
                || formals.contains(name) || !formals.is_disjoint(&local_names) {
                return Err(error("lambda parameters must match the interface arity and lexical bindings"));
            }
            let mut free = Names::new();
            reads(value, &mut free)?;
            let captured = free.difference(&formals).cloned().collect::<Vec<_>>();
            let mut helper_params = sam.params.iter().zip(lambda_params).map(|((ty, _), name)|
                (ty.clone(), name.clone())).collect::<Vec<_>>();
            for capture in &captured {
                if writes.contains(capture) || declarations.get(capture) != Some(&1) {
                    return Err(error("captured local is not effectively final"));
                }
                let ty = known.get(capture).filter(|ty| scalar(ty))
                    .ok_or_else(|| error("lambda capture must be an initialized scalar lexical binding"))?;
                helper_params.push((ty.clone(), capture.clone()));
            }
            // Angle brackets and colon cannot appear in a source identifier.
            let helper = format!("<lambda:{}>", helpers.len());
            let mut method = sam.clone();
            method.name = helper.clone();
            method.params = helper_params;
            method.is_static = true;
            method.is_public = false;
            method.is_private = true;
            method.body = Some(vec![JavaStatement::Return(Some(value.clone()))]);
            helpers.push(method);
            local_names.insert(name.clone());
            callables.insert(name.clone(), Callable { member: sam.name.clone(), helper,
                arity: lambda_params.len(), captured });
            continue;
        }
        walk(std::slice::from_mut(&mut stmt), &mut |expr| rewrite(expr, &callables), &mut |_| {})?;
        if let JavaStatement::VarDecl { java_type, name, initializer, .. } = &stmt {
            local_names.insert(name.clone());
            if initializer.is_some() { known.insert(name.clone(), java_type.clone()); }
        }
        lowered.push(stmt);
    }
    *body = lowered;
    Ok(())
}

fn reads(expr: &JavaExpr, out: &mut Names) -> Result<(), String> {
    use JavaExpr::*;
    match expr {
        Lit(_) => {}
        Ident(name) => { out.insert(name.clone()); }
        BinOp { left, right, .. } => { reads(left, out)?; reads(right, out)?; }
        UnaryOp { op: JavaUnaryOp::Neg | JavaUnaryOp::Not, operand } => reads(operand, out)?,
        Ternary { cond, then_expr, else_expr } => for value in [cond, then_expr, else_expr] { reads(value, out)?; },
        _ => return Err(error("lambda body requires a closed scalar expression")),
    }
    Ok(())
}

fn rewrite(expr: &mut JavaExpr, callables: &Callables) -> Result<(), String> {
    use JavaExpr::*;
    match expr {
        LocalLambda { .. } => return Err(error("local lambda must be a direct method-body declaration initializer")),
        Ident(name) if callables.contains_key(name) => return Err(error("local closures may only be invoked directly")),
        MethodCall { target, method, args } => {
            if let Some(Ident(name)) = target.as_deref() {
                if let Some(callable) = callables.get(name) {
                    if *method != callable.member || args.len() != callable.arity {
                        return Err(error("call does not match the local closure's declared SAM"));
                    }
                    *target = None;
                    *method = callable.helper.clone();
                    // Lexical primitive bindings are proved effectively final.
                    // Reading them after the real arguments cannot change the
                    // captured value or add an observable action.
                    args.extend(callable.captured.iter().cloned().map(Ident));
                }
            }
        }
        _ => {}
    }
    Ok(())
}

/// Pre-order walk: receiver calls can be consumed before visiting their
/// children. All other expression contexts see a normal use of the name.
fn walk(
    body: &mut [JavaStatement], visit: &mut impl FnMut(&mut JavaExpr) -> Result<(), String>,
    declare: &mut impl FnMut(&str),
) -> Result<(), String> {
    use JavaStatement::*;
    for stmt in body {
        match stmt {
            VarDecl { name, initializer, .. } => {
                declare(name);
                if let Some(expr) = initializer { walk_expr(expr, visit)?; }
            }
            Assign { target, value, .. } => {
                // Expose the native assignment's target to the write scan.
                let mut assignment = JavaExpr::Assign { target: Box::new(target.clone()),
                    value: Box::new(JavaExpr::Lit(JavaLit::Null)), is_compound: false };
                visit(&mut assignment)?;
                walk_expr(target, visit)?;
                walk_expr(value, visit)?;
            }
            ThisConstructor(args) => for arg in args { walk_expr(arg, visit)?; },
            ExprStmt(expr) | Throw(expr) | Return(Some(expr)) => walk_expr(expr, visit)?,
            If { cond, then_block, else_block } => {
                walk_expr(cond, visit)?;
                walk(then_block, visit, declare)?;
                if let Some(body) = else_block { walk(body, visit, declare)?; }
            }
            Switch { selector, cases, default, .. } => {
                walk_expr(selector, visit)?;
                for (labels, body) in cases {
                    for label in labels { walk_expr(label, visit)?; }
                    walk(body, visit, declare)?;
                }
                walk(default, visit, declare)?;
            }
            For { init, cond, update, body } => {
                if let Some(init) = init { walk(std::slice::from_mut(init.as_mut()), visit, declare)?; }
                if let Some(expr) = cond { walk_expr(expr, visit)?; }
                if let Some(expr) = update { walk_expr(expr, visit)?; }
                walk(body, visit, declare)?;
            }
            While { cond, body } => { walk_expr(cond, visit)?; walk(body, visit, declare)?; }
            ForEach { var_name, iterable, body, .. } => {
                declare(var_name);
                walk_expr(iterable, visit)?;
                walk(body, visit, declare)?;
            }
            Try { resource, body, catches, finally_block } => {
                if let Some(resource) = resource { walk(std::slice::from_mut(resource.as_mut()), visit, declare)?; }
                walk(body, visit, declare)?;
                for clause in catches { declare(&clause.var_name); walk(&mut clause.body, visit, declare)?; }
                if let Some(body) = finally_block { walk(body, visit, declare)?; }
            }
            Block(body) => walk(body, visit, declare)?,
            LocalIr(_) | Return(None) | LoopBreak | LoopContinue | SwitchBreak => {}
        }
    }
    Ok(())
}

fn walk_expr(expr: &mut JavaExpr, visit: &mut impl FnMut(&mut JavaExpr) -> Result<(), String>) -> Result<(), String> {
    use JavaExpr::*;
    visit(expr)?;
    match expr {
        Lit(_) | Ident(_) => {}
        LocalLambda { value, .. } | FieldAccess { target: value, .. } | UnaryOp { operand: value, .. }
        | InstanceOf { value, .. } | Cast { operand: value, .. } | ArrayAllocation { length: value, .. } =>
            walk_expr(value, visit)?,
        MethodCall { target, args, .. } => {
            if let Some(receiver) = target { walk_expr(receiver, visit)?; }
            for arg in args { walk_expr(arg, visit)?; }
        }
        New { args: elements, .. } | ArrayInitializer { elements, .. } =>
            for value in elements { walk_expr(value, visit)?; },
        Assign { target: left, value: right, .. } | BinOp { left, right, .. }
        | ArrayAccess { target: left, index: right } => {
            walk_expr(left, visit)?; walk_expr(right, visit)?;
        }
        Ternary { cond, then_expr, else_expr } =>
            for expr in [cond, then_expr, else_expr] { walk_expr(expr, visit)?; }
    }
    Ok(())
}
