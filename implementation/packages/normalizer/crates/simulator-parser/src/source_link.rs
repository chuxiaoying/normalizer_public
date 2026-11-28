//! Bind declared application types within the source package and import scope.
//! A simple type name never chooses a declaration in an unrelated package.
use crate::ast::*;
use std::{cell::RefCell, collections::{BTreeMap, BTreeSet}};

impl JavaProgram {
    /// Bind the closed source tree after its compilation units are known.
    /// Adapters that only parse a signature carrier do not use this linker.
    pub fn bind_source_names(&mut self) -> Result<(), String> {
        let qualify_names = self.classes.iter().any(|class| !class.package.is_empty());
        let units = self.classes.iter().map(|class| (class.full_name(), class.source_file.clone()))
            .collect::<BTreeMap<_, _>>();
        let packages = self.classes.iter().map(|class| (class.full_name(), class.package.clone()))
            .collect::<BTreeMap<_, _>>();
        // A direct qualifier and a unique name/arity can prove a private
        // selection only when there are no inherited overload candidates.
        let private_statics = self.classes.iter().filter(|class|
            class.supertype.is_none() && class.interfaces.is_empty()).map(|class| {
            let methods = class.methods.iter().filter(|method|
                method.is_private && method.is_static && !method.is_constructor
                && class.methods.iter().filter(|candidate| candidate.name == method.name
                    && candidate.params.len() == method.params.len()).count() == 1
            ).map(|method| (method.name.clone(), method.params.len())).collect::<BTreeSet<_>>();
            (class.full_name(), methods)
        }).collect::<BTreeMap<_, _>>();
        let nests = self.classes.iter().map(|class| {
            let top = class.simple_name.split('.').next().unwrap_or(&class.simple_name);
            let nest = if class.package.is_empty() { top.to_owned() } else { format!("{}.{top}", class.package) };
            (class.full_name(), nest)
        }).collect::<BTreeMap<_, _>>();
        let cross_package_statics = self.classes.iter().filter(|class| class.is_public_top_level())
            .map(|class| {
                let methods = class.methods.iter().filter(|method|
                    method.is_static && method.is_public && method.body.is_some()
                    && !method.is_constructor && method.type_parameters.is_empty()
                    && matches!(method.return_type, JavaType::Primitive(_) | JavaType::Void)
                    && method.params.iter().all(|(ty, _)| matches!(ty, JavaType::Primitive(_)))
                    && class.methods.iter().filter(|candidate| candidate.is_static
                        && candidate.name == method.name && candidate.params.len() == method.params.len()).count() == 1
                ).map(|method| (method.name.clone(), method.params.len())).collect::<BTreeSet<_>>();
                (class.full_name(), methods)
            }).collect::<BTreeMap<_, _>>();
        for class in &mut self.classes {
            let owner = class.full_name();
            let mut imports = BTreeMap::<String, String>::new();
            for import in &class.imports {
                let Some((_, short)) = import.rsplit_once('.') else { continue; };
                if qualify_names && (imports.get(short).is_some_and(|prior| prior != import)
                    || units.iter().any(|(defined, file)| file == &class.source_file
                        && defined.rsplit('.').next() == Some(short) && defined != import
                        && defined.strip_prefix(&class.package)
                            .unwrap_or(defined).trim_start_matches('.').split('.').count() == 1))
                {
                    return Err(format!("{}: conflicting source type import for {short}", class.source_file));
                }
                imports.insert(short.to_owned(), import.clone());
            }
            let issue = RefCell::new(None);
            let scope = Names { owner: &owner, package: &class.package, imports: &imports, packages: &packages,
                qualify_names, private_statics: &private_statics, nests: &nests,
                cross_package_statics: &cross_package_statics, issue: &issue };
            let types = class.type_parameters.iter().cloned().collect::<BTreeSet<_>>();
            let member_names = class.fields.iter().map(|field| field.name.clone()).collect::<BTreeSet<_>>();
            if let Some(parent) = &mut class.supertype { scope.bind_name(parent, &types); }
            for interface in &mut class.interfaces { scope.bind_name(interface, &types); }
            class.interface_arguments = std::mem::take(&mut class.interface_arguments).into_iter()
                .map(|(mut name, mut args)| {
                    scope.bind_name(&mut name, &types);
                    for arg in &mut args { scope.ty(arg, &types); }
                    (name, args)
                }).collect();
            for field in &mut class.fields {
                scope.ty(&mut field.java_type, &types);
                if let Some(value) = &mut field.initializer { scope.expr(value, &types, &member_names); }
            }
            for (_, body) in &mut class.static_initializers {
                scope.block(body, &types, &mut member_names.clone());
            }
            for method in &mut class.methods {
                let mut method_types = types.clone();
                method_types.extend(method.type_parameters.iter().cloned());
                let mut values = member_names.clone();
                for (parameter, name) in &mut method.params {
                    scope.ty(parameter, &method_types);
                    values.insert(name.clone());
                }
                scope.ty(&mut method.return_type, &method_types);
                for exception in &mut method.throws { scope.bind_name(exception, &method_types); }
                if let Some(body) = &mut method.body { scope.block(body, &method_types, &mut values); }
            }
            if let Some(message) = issue.into_inner() {
                return Err(format!("{}: {message}", class.source_file));
            }
        }
        Ok(())
    }
}

struct Names<'a> {
    owner: &'a str,
    package: &'a str,
    imports: &'a BTreeMap<String, String>,
    packages: &'a BTreeMap<String, String>,
    qualify_names: bool,
    private_statics: &'a BTreeMap<String, BTreeSet<(String, usize)>>,
    nests: &'a BTreeMap<String, String>,
    cross_package_statics: &'a BTreeMap<String, BTreeSet<(String, usize)>>,
    issue: &'a RefCell<Option<String>>,
}

impl Names<'_> {
    fn declared_name(&self, raw: &str) -> Option<String> {
        // An enclosing type may contribute a nested type name. Stop
        // at the package; a parent package is not a lexical scope.
        let mut owner = self.owner;
        while owner != self.package && !owner.is_empty() {
            let nested = format!("{owner}.{raw}");
            if self.packages.contains_key(&nested) { return Some(nested); }
            owner = owner.rsplit_once('.').map_or("", |(outer, _)| outer);
        }
        let (short, tail) = raw.split_once('.').map_or((raw, ""), |(head, tail)| (head, tail));
        if let Some(import) = self.imports.get(short) {
            let candidate = if tail.is_empty() { import.clone() } else { format!("{import}.{tail}") };
            // Only project declarations bind here; built-in imports are
            // handled by the native type gate rather than inferred by name.
            return self.packages.get_key_value(&candidate).map(|(declared, _)| declared.clone());
        }
        let in_package = if self.package.is_empty() { raw.to_owned() }
            else { format!("{}.{raw}", self.package) };
        if self.packages.contains_key(&in_package) { return Some(in_package); }
        if raw.contains('.') && self.packages.contains_key(raw) { return Some(raw.into()); }
        None
    }

    fn require_member_access(&self, bound: &str, call: Option<(&str, usize)>) {
        // Callers only reach this check with a key just resolved from
        // the declarations table. Every declaration owns a package and nest.
        if self.packages[bound] == self.package {
            if let Some((member, arity)) = call {
                if self.nests[self.owner] != self.nests[bound]
                    && self.private_statics.get(bound).is_some_and(|methods|
                        methods.contains(&(member.to_owned(), arity)))
                {
                    self.issue.borrow_mut().get_or_insert_with(||
                        format!("private member {bound}.{member} is not accessible from {}", self.owner));
                }
            }
            return;
        }
        // Cross-package references currently pass only at an unambiguous
        // direct call to a public top-level scalar method. Type references,
        // fields and instance receivers still need distinct access evidence.
        if call.is_some_and(|(member, arity)| self.cross_package_statics.get(bound)
            .is_some_and(|methods| methods.contains(&(member.to_owned(), arity))))
        {
            return;
        }
        self.issue.borrow_mut().get_or_insert_with(|| {
            let target = call.map_or_else(|| bound.to_owned(), |(member, _)| format!("{bound}.{member}"));
            format!("cross-package access to {target} requires a member access proof")
        });
    }

    fn bind_name(&self, name: &mut String, type_parameters: &BTreeSet<String>) {
        if !self.qualify_names || type_parameters.contains(name) { return; }
        if let Some(bound) = self.declared_name(name) {
            self.require_member_access(&bound, None);
            *name = bound;
        } else if matches!(name.as_str(), "Object" | "String") &&
            self.imports.get(name).is_none_or(|path| path.strip_prefix("java.lang.") == Some(name.as_str()))
        {
            // The implicit java.lang import is considered only after lexical
            // and explicit application bindings. Another package's short
            // name cannot shadow it.
            *name = format!("java.lang.{name}");
        } else if !self.package.is_empty() && self.packages.contains_key(name) && !name.contains('.') {
            // Named packages cannot refer to the unnamed package.
            *name = format!("{}.{name}", self.package);
        }
    }

    fn ty(&self, ty: &mut JavaType, types: &BTreeSet<String>) {
        match ty {
            JavaType::Class(name) => self.bind_name(name, types),
            JavaType::Generic(name, args) => {
                self.bind_name(name, types);
                for arg in args { self.ty(arg, types); }
            }
            JavaType::Array(item) => self.ty(item, types),
            JavaType::Primitive(_) | JavaType::Void => {}
        }
    }

    fn receiver(
        &self, value: &mut JavaExpr, types: &BTreeSet<String>, locals: &BTreeSet<String>,
        call: Option<(&str, usize)>,
    ) {
        fn path(expr: &JavaExpr) -> Option<String> {
            match expr {
                JavaExpr::Ident(name) => Some(name.clone()),
                JavaExpr::FieldAccess { target, field } => Some(format!("{}.{field}", path(target)?)),
                _ => None,
            }
        }
        if let Some(raw) = path(value) {
            let first = raw.split('.').next().unwrap_or(&raw);
            if !locals.contains(first) && !types.contains(first) && first != "this" {
                if let Some(bound) = self.declared_name(&raw) {
                    self.require_member_access(&bound, call);
                    if self.qualify_names { *value = JavaExpr::Ident(bound); }
                    return;
                }
                if !self.package.is_empty() && self.packages.contains_key(&raw) && !raw.contains('.') {
                    *value = JavaExpr::Ident(format!("{}.{raw}", self.package));
                    return;
                }
            }
        }
        self.expr(value, types, locals);
    }

    fn expr(&self, value: &mut JavaExpr, types: &BTreeSet<String>, locals: &BTreeSet<String>) {
        use JavaExpr::*;
        match value {
            Lit(_) | Ident(_) => {}
            InstanceOf {value, target, ..} | Cast {operand: value, target_type: target} => {
                self.ty(target, types);
                self.expr(value, types, locals);
            }
            FieldAccess {target, ..} => self.receiver(target, types, locals, None),
            MethodCall {target, method, args} => {
                if let Some(target) = target { self.receiver(target, types, locals, Some((method, args.len()))); }
                for arg in args { self.expr(arg, types, locals); }
            }
            New {java_type: element_type, args: elements} | ArrayInitializer {element_type, elements} => {
                self.ty(element_type, types);
                for item in elements { self.expr(item, types, locals); }
            }
            ArrayAllocation {element_type, length} => {
                self.ty(element_type, types);
                self.expr(length, types, locals);
            }
            BinOp {left, right, ..} | Assign {target: left, value: right, ..}
            | ArrayAccess {target: left, index: right} => {
                self.expr(left, types, locals);
                self.expr(right, types, locals);
            }
            UnaryOp {operand, ..} => self.expr(operand, types, locals),
            Ternary {cond, then_expr, else_expr} => {
                for child in [cond, then_expr, else_expr] { self.expr(child, types, locals); }
            }
        }
    }

    fn block(&self, body: &mut [JavaStatement], types: &BTreeSet<String>, locals: &mut BTreeSet<String>) {
        use JavaStatement::*;
        for stmt in body {
            match stmt {
                VarDecl {java_type, initializer, name, ..} => {
                    self.ty(java_type, types);
                    locals.insert(name.clone());
                    if let Some(value) = initializer { self.expr(value, types, locals); }
                }
                Assign {target, value, ..} => {
                    self.expr(target, types, locals);
                    self.expr(value, types, locals);
                }
                ThisConstructor(args) => for arg in args { self.expr(arg, types, locals); },
                ExprStmt(value) | Throw(value) | Return(Some(value)) => self.expr(value, types, locals),
                If {cond, then_block, else_block} => {
                    self.expr(cond, types, locals);
                    let mut yes = locals.clone();
                    let mut no = locals.clone();
                    if let Some(pattern) = cond.direct_type_pattern() {
                        let arm = if pattern.matched_when_true { &mut yes } else { &mut no };
                        arm.insert(pattern.name.to_owned());
                        if JavaStatement::pattern_in_continuation(
                            pattern.matched_when_true, then_block, else_block.as_deref(),
                        ) {
                            locals.insert(pattern.name.to_owned());
                        }
                    }
                    self.block(then_block, types, &mut yes);
                    if let Some(body) = else_block { self.block(body, types, &mut no); }
                }
                Switch {selector, cases, default} => {
                    self.expr(selector, types, locals);
                    for (label, body) in cases {
                        self.expr(label, types, locals);
                        self.block(body, types, &mut locals.clone());
                    }
                    self.block(default, types, &mut locals.clone());
                }
                While {cond, body} => {
                    self.expr(cond, types, locals);
                    self.block(body, types, &mut locals.clone());
                }
                For {init, cond, update, body} => {
                    let mut for_locals = locals.clone();
                    if let Some(init) = init {
                        match init.as_mut() {
                            Block(declarations) => self.block(declarations, types, &mut for_locals),
                            other => self.block(std::slice::from_mut(other), types, &mut for_locals),
                        }
                    }
                    for part in [cond, update].into_iter().flatten() { self.expr(part, types, &for_locals); }
                    self.block(body, types, &mut for_locals);
                }
                ForEach {var_type, var_name, iterable, body} => {
                    self.ty(var_type, types);
                    self.expr(iterable, types, locals);
                    let mut body_locals = locals.clone();
                    body_locals.insert(var_name.clone());
                    self.block(body, types, &mut body_locals);
                }
                Try {resource, body, catches, finally_block} => {
                    let mut protected = locals.clone();
                    if let Some(resource) = resource {
                        self.block(std::slice::from_mut(resource.as_mut()), types, &mut protected);
                    }
                    self.block(body, types, &mut protected);
                    for catch in catches {
                        let mut caught = locals.clone();
                        caught.insert(catch.var_name.clone());
                        for exception in &mut catch.exception_classes { self.bind_name(exception, types); }
                        self.block(&mut catch.body, types, &mut caught);
                    }
                    if let Some(body) = finally_block { self.block(body, types, &mut locals.clone()); }
                }
                Block(body) => self.block(body, types, &mut locals.clone()),
                LocalIr(_) | Return(None) | LoopBreak | LoopContinue => {}
            }
        }
    }
}
