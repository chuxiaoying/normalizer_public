//! Source linking for non-escaping local scalar SAMs. No closure value is
//! published. The certified callee gets ordinary Java argument evaluation
//! and completion; defining the lambda does not run that callee.
use crate::ast::*;
use std::collections::{BTreeMap, BTreeSet};

type Names = BTreeSet<String>;
type Callables = BTreeMap<String, Callable>;
type SourceMembers = BTreeMap<(String, String, usize), SourceSignature>;

struct SourceSignature {
    package: String,
    parameters: Vec<JavaType>,
    returns: JavaType,
}

#[derive(Clone)]
struct Callable {
    member: String,
    helper: String,
    arity: usize,
    captured: Vec<String>,
}

fn error(message: &str) -> String {
    format!("[JAVA-LOCAL-LAMBDA] {message}")
}

fn scalar(ty: &JavaType) -> bool {
    matches!(ty, JavaType::Primitive(name) if matches!(name.as_str(), "int" | "long" | "boolean"))
}

pub(crate) fn link(program: &mut JavaProgram) -> Result<(), String> {
    let interfaces: BTreeMap<_, _> = program
        .classes
        .iter()
        .filter_map(|class| {
            if !class.is_interface
                || class.supertype.is_some()
                || !class.interfaces.is_empty()
                || !class.type_parameters.is_empty()
                || !class.fields.is_empty()
            {
                return None;
            }
            let [sam] = class.methods.as_slice() else {
                return None;
            };
            (!sam.is_static
                && !sam.is_private
                && !sam.is_native
                && !sam.is_constructor
                && sam.body.is_none()
                && sam.type_parameters.is_empty()
                && sam.throws.is_empty()
                && scalar(&sam.return_type)
                && sam.params.iter().all(|(ty, _)| scalar(ty)))
            .then(|| (class.full_name(), sam.clone()))
        })
        .collect();
    let mut source_members = SourceMembers::new();
    for class in program.classes.iter().filter(|class| {
        !class.is_interface && class.supertype.is_none() && class.type_parameters.is_empty()
    }) {
        for method in &class.methods {
            if method.is_static
                || method.is_private
                || method.is_native
                || method.is_constructor
                || method.body.is_none()
                || !method.type_parameters.is_empty()
                || !method.throws.is_empty()
                || !scalar(&method.return_type)
                || !method.params.iter().all(|(ty, _)| scalar(ty))
            {
                continue;
            }
            if class
                .methods
                .iter()
                .filter(|candidate| {
                    candidate.name == method.name && candidate.params.len() == method.params.len()
                })
                .count()
                == 1
            {
                source_members.insert(
                    (class.full_name(), method.name.clone(), method.params.len()),
                    SourceSignature {
                        package: class.package.clone(),
                        parameters: method.params.iter().map(|(ty, _)| ty.clone()).collect(),
                        returns: method.return_type.clone(),
                    },
                );
            }
        }
    }
    for class in &mut program.classes {
        let mut helpers = Vec::new();
        for method in &mut class.methods {
            if let Some(body) = &mut method.body {
                lower(
                    body,
                    &method.params,
                    &interfaces,
                    &source_members,
                    &class.package,
                    &mut helpers,
                )?;
            }
        }
        class.methods.extend(helpers);
    }
    Ok(())
}

fn lower(
    body: &mut Vec<JavaStatement>,
    parameters: &[(JavaType, String)],
    interfaces: &BTreeMap<String, JavaMethod>,
    source_members: &SourceMembers,
    caller_package: &str,
    helpers: &mut Vec<JavaMethod>,
) -> Result<(), String> {
    let mut declarations = BTreeMap::<String, usize>::new();
    let mut captured_names = Names::new();
    for stmt in body.iter() {
        if let JavaStatement::VarDecl {
            initializer: Some(JavaExpr::LocalLambda { parameters, value }),
            ..
        } = stmt
        {
            let mut free = Names::new();
            body_reads(value, &mut free)?;
            captured_names.extend(free.into_iter().filter(|name| !parameters.contains(name)));
        }
    }
    let mut found = false;
    walk(
        body,
        &mut |expr| {
            match expr {
                JavaExpr::LocalLambda { .. } | JavaExpr::LocalBoundMethodReference { .. } => {
                    found = true
                }
                JavaExpr::InstanceOf {
                    binding: Some(name),
                    ..
                } => *declarations.entry(name.clone()).or_default() += 1,
                _ => {}
            }
            Ok(())
        },
        &mut |_| {},
    )?;
    if !found {
        return Ok(());
    }
    walk(body, &mut |_| Ok(()), &mut |name| {
        *declarations.entry(name.to_owned()).or_default() += 1
    })?;
    for (_, name) in parameters {
        *declarations.entry(name.clone()).or_default() += 1;
    }
    initialize_captured_blanks(body, &captured_names, &declarations)?;
    // Check effective finality over the remaining lexical method,
    // including writes made after lambda creation or in other branches.
    let mut writes = Names::new();
    walk(
        body,
        &mut |expr| {
            match expr {
                JavaExpr::Assign { target, .. }
                | JavaExpr::UnaryOp {
                    op:
                        JavaUnaryOp::PreInc
                        | JavaUnaryOp::PreDec
                        | JavaUnaryOp::PostInc
                        | JavaUnaryOp::PostDec,
                    operand: target,
                } => {
                    if let JavaExpr::Ident(name) = target.as_ref() {
                        writes.insert(name.clone());
                    }
                }
                _ => {}
            }
            Ok(())
        },
        &mut |_| {},
    )?;
    let mut known = parameters
        .iter()
        .map(|(ty, name)| (name.clone(), ty.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut local_names = parameters
        .iter()
        .map(|(_, name)| name.clone())
        .collect::<Names>();
    let mut callables = Callables::new();
    let mut lowered = Vec::with_capacity(body.len());
    for mut stmt in std::mem::take(body) {
        if let JavaStatement::VarDecl {
            java_type: JavaType::Class(target),
            name,
            initializer: Some(JavaExpr::LocalBoundMethodReference { receiver, member }),
            ..
        } = &stmt
        {
            if declarations.get(name) != Some(&1) || writes.contains(name) {
                return Err(error("local closure may not be shadowed or rebound"));
            }
            if declarations.get(receiver) != Some(&1) {
                return Err(error(
                    "method reference receiver needs a unique lexical binding",
                ));
            }
            let sam = interfaces.get(target).ok_or_else(|| {
                error("local method reference target must be a source-declared scalar SAM")
            })?;
            let Some(receiver_type @ JavaType::Class(owner)) = known.get(receiver) else {
                return Err(error(
                    "method reference needs an initialized source-class receiver",
                ));
            };
            let member_signature = source_members
                .get(&(owner.clone(), member.clone(), sam.params.len()))
                .ok_or_else(|| {
                    error("method reference needs a unique closed instance declaration")
                })?;
            if member_signature.package != caller_package
                || member_signature.returns != sam.return_type
                || !member_signature
                    .parameters
                    .iter()
                    .eq(sam.params.iter().map(|(ty, _)| ty))
            {
                return Err(error(
                    "method reference needs a visible exact scalar signature",
                ));
            }

            // Bound references acquire a receiver once on this path. Unlike
            // a lambda dereference, null fails even when the SAM never runs.
            let snapshot = format!("<bound:{}>", helpers.len());
            lowered.push(JavaStatement::VarDecl {
                java_type: receiver_type.clone(),
                name: snapshot.clone(),
                initializer: Some(JavaExpr::Ident(receiver.clone())),
                is_final: true,
            });
            lowered.push(JavaStatement::If {
                cond: JavaExpr::BinOp {
                    op: JavaBinOp::Eq,
                    left: Box::new(JavaExpr::Ident(snapshot.clone())),
                    right: Box::new(JavaExpr::Lit(JavaLit::Null)),
                },
                then_block: vec![JavaStatement::Throw(JavaExpr::New {
                    java_type: JavaType::Class("java.lang.NullPointerException".into()),
                    args: Vec::new(),
                })],
                else_block: None,
            });
            let helper = format!("<lambda:{}>", helpers.len());
            let mut parameters = sam.params.clone();
            let args = parameters
                .iter()
                .map(|(_, name)| JavaExpr::Ident(name.clone()))
                .collect();
            parameters.push((receiver_type.clone(), snapshot.clone()));
            helpers.push(make_helper(
                sam,
                &helper,
                parameters,
                JavaExpr::MethodCall {
                    target: Some(Box::new(JavaExpr::Ident(snapshot.clone()))),
                    method: member.clone(),
                    args,
                },
            ));
            local_names.insert(name.clone());
            callables.insert(
                name.clone(),
                Callable {
                    member: sam.name.clone(),
                    helper,
                    arity: sam.params.len(),
                    captured: vec![snapshot],
                },
            );
            continue;
        }
        let lambda = match &stmt {
            JavaStatement::VarDecl {
                java_type,
                name,
                initializer: Some(JavaExpr::LocalLambda { parameters, value }),
                ..
            } => Some((java_type, name, parameters, value.as_ref())),
            _ => None,
        };
        if let Some((ty, name, lambda_params, value)) = lambda {
            if declarations.get(name) != Some(&1) || writes.contains(name) {
                return Err(error("local closure may not be shadowed or rebound"));
            }
            let sam = match ty {
                JavaType::Class(owner) => interfaces.get(owner),
                _ => None,
            }
            .ok_or_else(|| error("local closure target must be a source-declared scalar SAM"))?;
            let formals: Names = lambda_params.iter().cloned().collect();
            if formals.len() != lambda_params.len()
                || lambda_params.len() != sam.params.len()
                || formals.contains(name)
                || !formals.is_disjoint(&local_names)
            {
                return Err(error(
                    "lambda parameters must match the interface arity and lexical bindings",
                ));
            }
            let mut free = Names::new();
            body_reads(value, &mut free)?;
            let reference = reference_body(
                value,
                &known,
                &helper_formals(sam, lambda_params),
                &sam.return_type,
                source_members,
                caller_package,
            )?;
            let captured = free.difference(&formals).cloned().collect::<Vec<_>>();
            let mut helper_params = sam
                .params
                .iter()
                .zip(lambda_params)
                .map(|((ty, _), name)| (ty.clone(), name.clone()))
                .collect::<Vec<_>>();
            for capture in &captured {
                if writes.contains(capture) || declarations.get(capture) != Some(&1) {
                    return Err(error("captured local is not effectively final"));
                }
                let ty = known
                    .get(capture)
                    .filter(|ty| scalar(ty) || reference == Some(capture.as_str()))
                    .ok_or_else(|| {
                        error("lambda capture must be an initialized scalar lexical binding")
                    })?;
                helper_params.push((ty.clone(), capture.clone()));
            }
            // Angle brackets and colon cannot appear in a source identifier.
            let helper = format!("<lambda:{}>", helpers.len());
            helpers.push(make_helper(sam, &helper, helper_params, value.clone()));
            local_names.insert(name.clone());
            callables.insert(
                name.clone(),
                Callable {
                    member: sam.name.clone(),
                    helper,
                    arity: lambda_params.len(),
                    captured,
                },
            );
            continue;
        }
        walk(
            std::slice::from_mut(&mut stmt),
            &mut |expr| rewrite(expr, &callables),
            &mut |_| {},
        )?;
        if let JavaStatement::VarDecl {
            java_type,
            name,
            initializer,
            ..
        } = &stmt
        {
            local_names.insert(name.clone());
            if initializer.is_some() {
                known.insert(name.clone(), java_type.clone());
            }
        }
        lowered.push(stmt);
    }
    *body = lowered;
    Ok(())
}

fn make_helper(
    sam: &JavaMethod,
    name: &str,
    params: Vec<(JavaType, String)>,
    value: JavaExpr,
) -> JavaMethod {
    let mut method = sam.clone();
    method.name = name.to_owned();
    method.params = params;
    method.is_static = true;
    method.is_public = false;
    method.is_private = true;
    method.body = Some(vec![JavaStatement::Return(Some(value))]);
    method
}

/// Keep the assignment at its original execution point. A blank declaration
/// is carried to exactly one simple assignment, or to a complete if/else
/// whose arms each consist of just that assignment. Prior unrelated
/// statements stay in place. A unique declaration and a no-use prefix show
/// that moving the declaration itself is not observable in this subset.
fn initialize_captured_blanks(
    body: &mut Vec<JavaStatement>,
    captures: &Names,
    declarations: &BTreeMap<String, usize>,
) -> Result<(), String> {
    use JavaStatement::*;
    let mut index = 0;
    while index < body.len() {
        let VarDecl {
            java_type,
            name,
            initializer: None,
            ..
        } = &body[index]
        else {
            index += 1;
            continue;
        };
        if !captures.contains(name) || declarations.get(name) != Some(&1) || !scalar(java_type) {
            index += 1;
            continue;
        }
        let (ty, name) = (java_type.clone(), name.clone());
        let mut found = None;
        for (i, stmt) in body.iter_mut().enumerate().skip(index + 1) {
            let mut mentioned = false;
            walk(
                std::slice::from_mut(stmt),
                &mut |expression| {
                    if matches!(expression, JavaExpr::Ident(id) if id == &name) {
                        mentioned = true;
                    }
                    Ok(())
                },
                &mut |_| {},
            )?;
            if mentioned {
                found = Some(i);
                break;
            }
        }
        let at = found.ok_or_else(|| error("captured blank local is not definitely assigned"))?;
        let value = match &body[at] {
            stmt if simple_initial_assignment(stmt, &name).is_some() => {
                simple_initial_assignment(stmt, &name).unwrap().clone()
            }
            If {
                cond,
                then_block,
                else_block: Some(other),
            } => {
                let ([yes], [no]) = (then_block.as_slice(), other.as_slice()) else {
                    return Err(error("captured blank local is not definitely assigned by a closed initialization"));
                };
                if mentions(cond, &name) {
                    return Err(error("captured blank local is read before assignment"));
                }
                let yes = simple_initial_assignment(yes, &name).ok_or_else(|| {
                    error("captured blank local is not definitely assigned in both if arms")
                })?;
                let no = simple_initial_assignment(no, &name).ok_or_else(|| {
                    error("captured blank local is not definitely assigned in both if arms")
                })?;
                JavaExpr::Ternary {
                    cond: Box::new(cond.clone()),
                    then_expr: Box::new(yes.clone()),
                    else_expr: Box::new(no.clone()),
                }
            }
            _ => return Err(error("captured blank local is not definitely assigned")),
        };
        body[at] = VarDecl {
            java_type: ty,
            name,
            initializer: Some(value),
            is_final: false,
        };
        body.remove(index);
    }
    Ok(())
}

fn mentions(value: &JavaExpr, name: &str) -> bool {
    let mut found = false;
    let mut copy = value.clone();
    let _ = walk_expr(&mut copy, &mut |e| {
        if matches!(e, JavaExpr::Ident(id) if id == name) {
            found = true;
        }
        Ok(())
    });
    found
}

fn simple_initial_assignment<'a>(stmt: &'a JavaStatement, name: &str) -> Option<&'a JavaExpr> {
    let JavaStatement::Assign {
        target: JavaExpr::Ident(target),
        value,
        is_compound: false,
    } = stmt
    else {
        return None;
    };
    (target == name && !mentions(value, name)).then_some(value)
}

fn helper_formals(sam: &JavaMethod, parameters: &[String]) -> BTreeMap<String, JavaType> {
    sam.params
        .iter()
        .zip(parameters)
        .map(|((ty, _), name)| (name.clone(), ty.clone()))
        .collect()
}

/// A source reference is admitted only as the receiver of a closed, visible
/// instance call. The helper keeps the original receiver identity and closure
/// invocation path; this is not a new public function or an external port.
fn reference_body<'a>(
    value: &'a JavaExpr,
    known: &BTreeMap<String, JavaType>,
    formals: &BTreeMap<String, JavaType>,
    result: &JavaType,
    members: &SourceMembers,
    caller_package: &str,
) -> Result<Option<&'a str>, String> {
    let JavaExpr::MethodCall {
        target: Some(target),
        method,
        args,
    } = value
    else {
        return Ok(None);
    };
    let JavaExpr::Ident(receiver) = target.as_ref() else {
        return Err(error("lambda receiver must be a lexical source binding"));
    };
    let Some(JavaType::Class(owner)) = known.get(receiver) else {
        return Err(error(
            "lambda receiver must be an initialized source-class binding",
        ));
    };
    let Some(member) = members.get(&(owner.clone(), method.clone(), args.len())) else {
        return Err(error(
            "lambda receiver call needs a unique closed instance declaration",
        ));
    };
    if member.package != caller_package
        || member.returns != *result
        || !member.parameters.iter().zip(args).all(
            |(ty, arg)| matches!(arg, JavaExpr::Ident(formal) if formals.get(formal) == Some(ty)),
        )
    {
        return Err(error(
            "lambda receiver call needs a visible exact scalar signature",
        ));
    }
    Ok(Some(receiver))
}

fn body_reads(expr: &JavaExpr, out: &mut Names) -> Result<(), String> {
    if let JavaExpr::MethodCall {
        target: Some(target),
        args,
        ..
    } = expr
    {
        reads(target, out)?;
        for arg in args {
            reads(arg, out)?;
        }
        return Ok(());
    }
    reads(expr, out)
}

fn reads(expr: &JavaExpr, out: &mut Names) -> Result<(), String> {
    use JavaExpr::*;
    match expr {
        Lit(_) => {}
        Ident(name) => {
            out.insert(name.clone());
        }
        BinOp { left, right, .. } => {
            reads(left, out)?;
            reads(right, out)?;
        }
        UnaryOp {
            op: JavaUnaryOp::Neg | JavaUnaryOp::Not,
            operand,
        } => reads(operand, out)?,
        Ternary {
            cond,
            then_expr,
            else_expr,
        } => {
            for value in [cond, then_expr, else_expr] {
                reads(value, out)?;
            }
        }
        _ => return Err(error("lambda body requires a closed scalar expression")),
    }
    Ok(())
}

fn rewrite(expr: &mut JavaExpr, callables: &Callables) -> Result<(), String> {
    use JavaExpr::*;
    match expr {
        LocalLambda { .. } => {
            return Err(error(
                "local lambda must be a direct method-body declaration initializer",
            ))
        }
        LocalBoundMethodReference { .. } => {
            return Err(error(
                "bound method reference must be a direct method-body declaration initializer",
            ))
        }
        Ident(name) if callables.contains_key(name) => {
            return Err(error("local closures may only be invoked directly"))
        }
        MethodCall {
            target,
            method,
            args,
        } => {
            if let Some(Ident(name)) = target.as_deref() {
                if let Some(callable) = callables.get(name) {
                    if *method != callable.member || args.len() != callable.arity {
                        return Err(error(
                            "call does not match the local closure's declared SAM",
                        ));
                    }
                    *target = None;
                    *method = callable.helper.clone();
                    // Lexical bindings are proved effectively final.
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
    body: &mut [JavaStatement],
    visit: &mut impl FnMut(&mut JavaExpr) -> Result<(), String>,
    declare: &mut impl FnMut(&str),
) -> Result<(), String> {
    use JavaStatement::*;
    for stmt in body {
        match stmt {
            VarDecl {
                name, initializer, ..
            } => {
                declare(name);
                if let Some(expr) = initializer {
                    walk_expr(expr, visit)?;
                }
            }
            Assign { target, value, .. } => {
                // Expose the native assignment's target to the write scan.
                let mut assignment = JavaExpr::Assign {
                    target: Box::new(target.clone()),
                    value: Box::new(JavaExpr::Lit(JavaLit::Null)),
                    is_compound: false,
                };
                visit(&mut assignment)?;
                walk_expr(target, visit)?;
                walk_expr(value, visit)?;
            }
            ThisConstructor(args) => {
                for arg in args {
                    walk_expr(arg, visit)?;
                }
            }
            ExprStmt(expr) | Throw(expr) | Return(Some(expr)) => walk_expr(expr, visit)?,
            If {
                cond,
                then_block,
                else_block,
            } => {
                walk_expr(cond, visit)?;
                walk(then_block, visit, declare)?;
                if let Some(body) = else_block {
                    walk(body, visit, declare)?;
                }
            }
            Switch {
                selector,
                cases,
                default,
                ..
            } => {
                walk_expr(selector, visit)?;
                for (labels, body) in cases {
                    for label in labels {
                        walk_expr(label, visit)?;
                    }
                    walk(body, visit, declare)?;
                }
                walk(default, visit, declare)?;
            }
            For {
                init,
                cond,
                update,
                body,
            } => {
                if let Some(init) = init {
                    walk(std::slice::from_mut(init.as_mut()), visit, declare)?;
                }
                if let Some(expr) = cond {
                    walk_expr(expr, visit)?;
                }
                if let Some(expr) = update {
                    walk_expr(expr, visit)?;
                }
                walk(body, visit, declare)?;
            }
            While { cond, body } | DoWhile { cond, body } => {
                walk_expr(cond, visit)?;
                walk(body, visit, declare)?;
            }
            ForEach {
                var_name,
                iterable,
                body,
                ..
            } => {
                declare(var_name);
                walk_expr(iterable, visit)?;
                walk(body, visit, declare)?;
            }
            Try {
                resource,
                body,
                catches,
                finally_block,
            } => {
                if let Some(resource) = resource {
                    walk(std::slice::from_mut(resource.as_mut()), visit, declare)?;
                }
                walk(body, visit, declare)?;
                for clause in catches {
                    declare(&clause.var_name);
                    walk(&mut clause.body, visit, declare)?;
                }
                if let Some(body) = finally_block {
                    walk(body, visit, declare)?;
                }
            }
            Block(body) => walk(body, visit, declare)?,
            LocalIr(_)
            | Return(None)
            | LoopBreak
            | LoopContinue
            | SwitchBreak
            | DoOnceExit
            | ObjectSuperConstructor => {}
        }
    }
    Ok(())
}

fn walk_expr(
    expr: &mut JavaExpr,
    visit: &mut impl FnMut(&mut JavaExpr) -> Result<(), String>,
) -> Result<(), String> {
    use JavaExpr::*;
    visit(expr)?;
    match expr {
        Lit(_) | Ident(_) | LocalBoundMethodReference { .. } => {}
        LocalLambda { value, .. }
        | FieldAccess { target: value, .. }
        | UnaryOp { operand: value, .. }
        | InstanceOf { value, .. }
        | Cast { operand: value, .. }
        | ArrayAllocation { length: value, .. } => walk_expr(value, visit)?,
        MethodCall { target, args, .. } => {
            if let Some(receiver) = target {
                walk_expr(receiver, visit)?;
            }
            for arg in args {
                walk_expr(arg, visit)?;
            }
        }
        New { args: elements, .. } | ArrayInitializer { elements, .. } => {
            for value in elements {
                walk_expr(value, visit)?;
            }
        }
        Assign {
            target: left,
            value: right,
            ..
        }
        | BinOp { left, right, .. }
        | ArrayAccess {
            target: left,
            index: right,
        } => {
            walk_expr(left, visit)?;
            walk_expr(right, visit)?;
        }
        Ternary {
            cond,
            then_expr,
            else_expr,
        } => {
            for expr in [cond, then_expr, else_expr] {
                walk_expr(expr, visit)?;
            }
        }
    }
    Ok(())
}
