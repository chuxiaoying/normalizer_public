//! Source-level type and name evidence. This pass sees all branches and methods;
//! a concrete execution path is not evidence that the other source compiles.
use simulator_parser::{JavaBinOp as B, JavaClass, JavaExpr as E, JavaLit, JavaMethod,
    JavaProgram, JavaStatement as S, JavaType, JavaUnaryOp as U};
use std::collections::BTreeMap;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Number, Bool, Text, Null, Sequence, Ref, Unresolved,
}
impl Kind {
    fn declared(ty: &JavaType) -> Self {
        match ty {
            JavaType::Primitive(name) if name == "boolean" => Self::Bool,
            JavaType::Primitive(_) => Self::Number,
            JavaType::Class(name) if matches!(name.as_str(), "String" | "java.lang.String") => Self::Text,
            JavaType::Array(_) => Self::Sequence,
            JavaType::Class(_) | JavaType::Generic(..) => Self::Ref,
            JavaType::Void => Self::Unresolved,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Number => "number",
            Self::Bool => "bool",
            Self::Text => "text",
            Self::Null => "null",
            Self::Sequence => "source sequence<number>",
            Self::Ref => "reference",
            Self::Unresolved => "unresolved type",
        }
    }

    // Ref may have an implicit Java boxing conversion; do not guess a
    // concrete nominal type for it. Other known primitive families are disjoint.
    fn conflicts_with(self, other: Self) -> bool {
        if self == other || matches!(self, Self::Ref | Self::Unresolved)
            || matches!(other, Self::Ref | Self::Unresolved)
        {
            return false;
        }
        if self == Self::Null || other == Self::Null {
            return matches!(self, Self::Bool | Self::Number) || matches!(other, Self::Bool | Self::Number);
        }
        true
    }
}

type Scope = BTreeMap<String, JavaType>;

struct Admission<'a> {
    program: &'a JavaProgram,
    class: &'a JavaClass,
    method: &'a JavaMethod,
    id: &'a str,
}

impl Admission<'_> {
    fn diagnostic(&self, code: &str, detail: &str) -> String {
        crate::preprocess::required(
            code, self.id, detail,
            "请修正源码中的名称或静态类型；不要删除未执行的路径来绕过入口检查。",
        )
    }

    fn binding<'a>(&'a self, name: &str, scope: &'a Scope) -> Option<&'a JavaType> {
        scope.get(name).or_else(|| {
            self.class.fields().iter().find(|field| {
                field.name() == name && (!self.method.is_static() || field.is_static())
            }).map(|field| field.java_type())
        })
    }

    fn declared_type(&self, expr: &E, scope: &Scope) -> Option<JavaType> {
        match expr {
            E::Ident(name) if name == "this" && !self.method.is_static() =>
                Some(JavaType::Class(self.class.full_name())),
            E::Ident(name) => self.binding(name, scope).cloned(),
            E::New { java_type, .. } => Some(java_type.clone()),
            E::ArrayInitializer { element_type, .. } | E::ArrayAllocation { element_type, .. } =>
                Some(JavaType::Array(Box::new(element_type.clone()))),
            E::Cast { target_type, .. } => Some(target_type.clone()),
            E::FieldAccess { target, field } => {
                let JavaType::Class(owner) = self.declared_type(target, scope)? else { return None };
                self.program.find_class(&owner)?.fields().iter().find(|f| f.name() == field)
                    .map(|f| f.java_type().clone())
            }
            E::ArrayAccess { target, .. } => {
                let JavaType::Array(element) = self.declared_type(target, scope)? else { return None };
                Some(*element)
            }
            E::MethodCall { target, method, args } => {
                let owner = match target {
                    None => self.class,
                    Some(target) => {
                        let JavaType::Class(owner) = self.declared_type(target, scope)? else { return None };
                        self.program.find_class(&owner)?
                    }
                };
                owner.methods().iter().find(|m| m.name() == method && m.arity() == args.len())
                    .map(|m| m.return_type().clone())
            }
            _ => None,
        }
    }

    fn expression(&self, expr: &E, scope: &Scope) -> Result<Kind, String> {
        let declared = || self.declared_type(expr, scope).as_ref()
            .map_or(Kind::Unresolved, Kind::declared);
        Ok(match expr {
            E::Ident(name) => {
                if let Some(ty) = self.declared_type(expr, scope) {
                    Kind::declared(&ty)
                } else {
                    return Err(self.diagnostic("NORM-SOURCE-NAME", &format!("undeclared value {name}")));
                }
            }
            E::Lit(JavaLit::Bool(_)) => Kind::Bool,
            E::Lit(JavaLit::Int(_) | JavaLit::Long(_) | JavaLit::Char(_)) => Kind::Number,
            E::Lit(JavaLit::Str(_)) => Kind::Text,
            E::Lit(JavaLit::Null) => Kind::Null,
            E::FieldAccess { target, .. } => { self.expression(target, scope)?; declared() }
            E::MethodCall { args, .. } | E::New { args, .. } => {
                // Structural admission binds call targets, including external
                // receiver names; a syntactic qualifier is not a value read.
                for arg in args { self.expression(arg, scope)?; }
                declared()
            }
            E::ArrayInitializer { elements, .. } => {
                for element in elements { self.expression(element, scope)?; }
                Kind::Sequence
            }
            E::ArrayAllocation { length, .. } => {
                self.expression(length, scope)?;
                Kind::Sequence
            }
            E::BinOp { op, left, right } => {
                let left = self.expression(left, scope)?;
                let right = self.expression(right, scope)?;
                match op {
                    B::And | B::Or => {
                        self.condition(left, "left logical operand")?;
                        self.condition(right, "right logical operand")?;
                        Kind::Bool
                    }
                    B::Eq | B::Ne | B::Lt | B::Le | B::Gt | B::Ge => Kind::Bool,
                    B::Add if left == Kind::Text || right == Kind::Text => Kind::Text,
                    B::BitAnd | B::BitOr | B::BitXor if left == Kind::Bool && right == Kind::Bool => Kind::Bool,
                    _ if left == Kind::Number && right == Kind::Number => Kind::Number,
                    _ => Kind::Unresolved,
                }
            }
            E::Assign { target, value } => {
                let to = self.expression(target, scope)?;
                let from = self.expression(value, scope)?;
                self.assignment(target, to, from)?;
                to
            }
            E::UnaryOp { op, operand } => {
                let kind = self.expression(operand, scope)?;
                if *op == U::Not {
                    self.condition(kind, "not operand")?;
                    Kind::Bool
                } else {
                    kind
                }
            }
            E::Ternary { cond, then_expr, else_expr } => {
                self.condition(self.expression(cond, scope)?, "ternary condition")?;
                let yes = self.expression(then_expr, scope)?;
                let no = self.expression(else_expr, scope)?;
                if yes.conflicts_with(no) {
                    return Err(self.diagnostic("NORM-SOURCE-TYPE", &format!(
                        "branch result type mismatch: then={} else={}", yes.label(), no.label())));
                }
                if yes == no { yes } else { Kind::Unresolved }
            }
            E::Cast { operand, .. } => { self.expression(operand, scope)?; declared() }
            E::ArrayAccess { target, index } => {
                self.expression(target, scope)?;
                self.expression(index, scope)?;
                declared()
            }
        })
    }

    fn condition(&self, kind: Kind, label: &str) -> Result<(), String> {
        if kind != Kind::Bool {
            return Err(self.diagnostic("NORM-SOURCE-TYPE", &format!("{label} requires boolean, got {}", kind.label())));
        }
        Ok(())
    }

    fn compatible(&self, expected: Kind, actual: Kind, label: &str) -> Result<(), String> {
        if expected.conflicts_with(actual) {
            return Err(self.diagnostic("NORM-SOURCE-TYPE", &format!(
                "{label} requires {}, got {}", expected.label(), actual.label())));
        }
        Ok(())
    }

    fn assignment(&self, target: &E, expected: Kind, actual: Kind) -> Result<(), String> {
        let label = match target {
            E::Ident(name) => format!("assignment to {name}"),
            E::FieldAccess { field, .. } => format!("assignment to {field}"),
            _ => "assignment".into(),
        };
        self.compatible(expected, actual, &label)
    }

    fn block(&self, body: &[S], scope: &mut Scope) -> Result<(), String> {
        for statement in body {
            match statement {
                S::VarDecl { java_type, name, initializer } => {
                    if let Some(value) = initializer {
                        let actual = self.expression(value, scope)?;
                        self.compatible(Kind::declared(java_type), actual, &format!("initializer for {name}"))?;
                    }
                    scope.insert(name.clone(), java_type.clone());
                }
                S::Assign { target, value } => {
                    let expected = self.expression(target, scope)?;
                    let actual = self.expression(value, scope)?;
                    self.assignment(target, expected, actual)?;
                }
                S::Return(Some(value)) => {
                    let actual = self.expression(value, scope)?;
                    self.compatible(Kind::declared(self.method.return_type()), actual, "return")?;
                }
                S::If { cond, then_block, else_block } => {
                    self.condition(self.expression(cond, scope)?, "if condition")?;
                    self.block(then_block, &mut scope.clone())?;
                    if let Some(body) = else_block { self.block(body, &mut scope.clone())?; }
                }
                S::Try { body, catches, finally_block } => {
                    self.block(body, &mut scope.clone())?;
                    for handler in catches {
                        let mut handler_scope = scope.clone();
                        handler_scope.insert(handler.var_name().into(), JavaType::Class(handler.exception_class().into()));
                        self.block(handler.body(), &mut handler_scope)?;
                    }
                    if let Some(body) = finally_block { self.block(body, &mut scope.clone())?; }
                }
                S::Block(body) => self.block(body, &mut scope.clone())?,
                S::Throw(value) | S::ExprStmt(value) => { self.expression(value, scope)?; }
                // Fixed-loop headers and lifted directives have their own
                // source contract. Visit native bodies with their lexical bindings.
                S::For { init, body, .. } => {
                    let mut loop_scope = scope.clone();
                    if let Some(init) = init { self.block(std::slice::from_ref(init.as_ref()), &mut loop_scope)?; }
                    self.block(body, &mut loop_scope)?;
                }
                S::ForEach { var_type, var_name, iterable, body } => {
                    self.expression(iterable, scope)?;
                    let mut loop_scope = scope.clone();
                    loop_scope.insert(var_name.clone(), var_type.clone());
                    self.block(body, &mut loop_scope)?;
                }
                S::While { cond, body } => {
                    self.condition(self.expression(cond, scope)?, "while condition")?;
                    self.block(body, &mut scope.clone())?;
                }
                S::LocalIr(_) | S::Return(None) => {}
            }
        }
        Ok(())
    }
}

pub(super) fn validate(program: &JavaProgram, class: &JavaClass, method: &JavaMethod, id: &str, body: &[S]) -> Result<(), String> {
    let mut scope = method.params().iter().map(|(ty, name)| (name.clone(), ty.clone())).collect();
    Admission { program, class, method, id }.block(body, &mut scope)
}
