//! Source-level type and name evidence. This pass sees all branches and methods;
//! a concrete execution path is not evidence that the other source compiles.
use simulator_parser::{JavaBinOp as B, JavaClass, JavaExpr as E, JavaLit, JavaMethod,
    JavaProgram, JavaStatement as S, JavaType, JavaUnaryOp as U};
use std::collections::BTreeMap;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Number, Float, Double, DecimalCarrier, CalendarDate, Bool, Text, Null, Sequence, Ref, Unresolved,
}
impl Kind {
    fn declared(ty: &JavaType) -> Self {
        match ty {
            JavaType::Primitive(name) if name == "boolean" => Self::Bool,
            JavaType::Primitive(name) if name == "float" => Self::Float,
            JavaType::Primitive(name) if name == "double" => Self::Double,
            JavaType::Primitive(_) => Self::Number,
            t if super::exact_decimal_type(t) => Self::DecimalCarrier,
            t if super::calendar_date_type(t) => Self::CalendarDate,
            JavaType::Class(name) if matches!(name.as_str(), "String" | "java.lang.String") => Self::Text,
            JavaType::Array(_) => Self::Sequence,
            JavaType::Class(_) | JavaType::Generic(..) => Self::Ref,
            JavaType::Void => Self::Unresolved,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Number => "number",
            Self::Float => "float",
            Self::Double => "double",
            Self::Bool => "bool",
            Self::CalendarDate => "date",
            Self::Sequence => "source sequence<number>",
            _ => "source value",
        }
    }

    fn is_native_decimal(self) -> bool {
        matches!(self, Self::Float | Self::Double)
    }

    fn numeric_union(self, other: Self) -> Option<Self> {
        match (self, other) {
            (Self::Double, Self::Number | Self::Float | Self::Double)
            | (Self::Number | Self::Float, Self::Double) => Some(Self::Double),
            (Self::Float, Self::Number | Self::Float) | (Self::Number, Self::Float) => Some(Self::Float),
            (Self::Number, Self::Number) => Some(Self::Number),
            _ => None,
        }
    }

    // Ref may have an implicit Java boxing conversion; do not guess a
    // concrete nominal type for it. Other known primitive families are disjoint.
    fn conflicts_with(self, other: Self) -> bool {
        if self == other || self.numeric_union(other).is_some()
            || matches!(self, Self::Ref | Self::Unresolved)
            || matches!(other, Self::Ref | Self::Unresolved)
        {
            return false;
        }
        // Null is compatible only with the remaining nullable families.
        // Primitive kinds still fail source admission before evaluation.
        !matches!((self, other),
            (Self::Null, Self::Text | Self::Sequence | Self::DecimalCarrier)
            | (Self::Text | Self::Sequence | Self::DecimalCarrier, Self::Null))
    }
}

// This is only Java compile-time admission. Business execution never
// transforms a literal through either IEEE representation.
fn native_literal_compiles(digits: &str, radix: u32, power: i32, single: bool) -> bool {
    use math::Integer;
    let value = Integer::parse_bytes(digits.as_bytes(), radix)
        .expect("source parser has checked the numeric token");
    let zero = Integer::from(0);
    if value == zero {
        return true;
    }
    let one = Integer::from(1);
    let factor = Integer::from(if radix == 16 { 2 } else { 10 }).pow(power.unsigned_abs());
    let (num, den) = if power < 0 { (value, factor) } else { (value * factor, one.clone()) };
    let (max, precision, tiny): (usize, usize, usize) =
        if single { (128, 24, 150) } else { (1024, 53, 1075) };
    // At the exact midpoints, ties-to-even chooses infinity or zero.
    let overflow_midpoint = (&one << max) - (&one << (max - precision - 1));
    (&num << tiny) > den && num < overflow_midpoint * den
}

type Scope = BTreeMap<String, Binding>;

#[derive(Clone)]
struct Binding {
    ty: JavaType,
    initialized_final: bool,
    initialized_local: bool,
}

impl Binding {
    fn plain(ty: JavaType) -> Self {
        Self { ty, initialized_final: false, initialized_local: false }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum IntegerWidth {
    IntFamily,
    Long,
}

impl IntegerWidth {
    fn declared(ty: &JavaType) -> Option<Self> {
        match ty {
            JavaType::Primitive(name) if name == "long" => Some(Self::Long),
            JavaType::Primitive(name) if matches!(name.as_str(), "byte" | "short" | "char" | "int") =>
                Some(Self::IntFamily),
            _ => None,
        }
    }

    fn promote(self, other: Self) -> Self {
        if self == Self::Long || other == Self::Long { Self::Long } else { Self::IntFamily }
    }
}

struct Admission<'a> {
    program: &'a JavaProgram,
    class: &'a JavaClass,
    method: &'a JavaMethod,
    id: &'a str,
    layouts: &'a BTreeMap<String, BTreeMap<String, String>>,
    closed_receiver: bool,
    assembly: Option<&'a crate::kernel_input::SourceReceiver>,
}

impl Admission<'_> {
    fn diagnostic(&self, code: &str, detail: &str) -> String {
        crate::preprocess::required(
            code, self.id, detail,
            "请修正源码中的名称或静态类型；不要删除未执行的路径来绕过入口检查。",
        )
    }

    fn unmodeled_receiver_field(&self, field: &simulator_parser::JavaField) -> bool {
        self.closed_receiver && !self.method.is_static() && !self.method.is_constructor()
            && !field.is_static()
            && !super::entry_state(self.class, field) && !super::source_ports::marked(self.class, field)
            && !self.assembly.is_some_and(|r| r.dependency(&self.class.full_name(), field.name()).is_some())
    }

    fn check_receiver_use(&self, name: &str) -> Result<(), String> {
        if self.class.fields().iter().any(|f| f.name() == name && self.unmodeled_receiver_field(f)) {
            return Err(self.diagnostic("NORM-ENTRY-STATE", &format!(
                "receiver field {name} is used but has no entry-state or external binding")));
        }
        Ok(())
    }

    fn binding<'a>(&'a self, name: &str, scope: &'a Scope) -> Option<&'a JavaType> {
        scope.get(name).map(|binding| &binding.ty).or_else(|| {
            // Source layout and port admission do not allow static fields.
            if self.method.is_static() {
                return None;
            }
            self.class.fields().iter().find(|field| field.name() == name)
                .map(|field| field.java_type())
        })
    }

    fn method<'a>(&'a self, target: Option<&E>, method: &str, arity: usize, scope: &Scope) -> Option<&'a JavaMethod> {
        if let Some(target) = target {
            let root = match target {
                E::Ident(field) if !scope.contains_key(field) => Some((self.class, field)),
                E::FieldAccess { target, field } => match target.as_ref() {
                    E::Ident(name) if !scope.contains_key(name) && self.binding(name, scope).is_none() =>
                        self.program.find_class(name).map(|owner| (owner, field)),
                    _ => None,
                },
                _ => None,
            };
            if let Some((root, field)) = root {
                if let Some((_, method, _)) = crate::kernel_input::inert_instance_method(
                    self.program, self.class, root, field, method, arity,
                ) { return Some(method); }
            }
        }
        let owner = match target {
            None => self.class,
            Some(E::Ident(owner)) if self.binding(owner, scope).is_none() => self.program.find_class(owner)?,
            Some(target) => {
                let ty = self.declared_type(target, scope)?;
                match &ty {
                    JavaType::Class(owner) => self.program.find_class(owner)?,
                    _ => super::specialized_interface(self.program, &ty)?,
                }
            }
        };
        super::named_method_for_arity(owner, method, arity).map(|(_, member)| member)
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
                if field == "length" && self.declared_type(target, scope).is_some_and(|ty| matches!(ty, JavaType::Array(_))) {
                    return Some(JavaType::Primitive("int".into()));
                }
                let target = self.declared_type(target, scope)?;
                if let Some(product) = super::generic_product::instantiated(self.program, &target) {
                    return product.field_type(field);
                }
                let JavaType::Class(owner) = target else { return None };
                self.program.find_class(&owner)?.fields().iter().find(|f| f.name() == field)
                    .map(|f| f.java_type().clone())
            }
            E::ArrayAccess { target, .. } => {
                let JavaType::Array(element) = self.declared_type(target, scope)? else { return None };
                Some(*element)
            }
            E::MethodCall { target, method, args } => {
                if let Some(E::Ident(receiver)) = target.as_deref() {
                    if self.binding(receiver, scope).is_none()
                        && crate::normalized_text::clock_intrinsic(
                            self.program, self.class, &format!("{receiver}.{method}"), args.len(),
                        )
                    {
                        return Some(JavaType::Primitive("long".into()));
                    }
                }
                if let Some(target) = target.as_deref() {
                    if let Some(JavaType::Class(owner)) = self.declared_type(target, scope) {
                        if let Some((_, ty)) = self.program.find_class(&owner).and_then(|class|
                            super::source_record_accessor(class, method, args.len()))
                        { return Some(ty.clone()); }
                    }
                }
                self.method(target.as_deref(), method, args.len(), scope).map(|m| m.return_type().clone())
            }
            _ => None,
        }
    }

    fn instanceof_type(&self, value: &E, target: &JavaType, scope: &Scope) -> Result<Kind, String> {
        let kind = self.expression(value, scope)?;
        if super::platform_string_type(self.program, target) {
            if kind == Kind::Null || self.declared_type(value, scope).is_some_and(|source|
                self.root_object_target(&source) || super::platform_string_type(self.program, &source))
                || matches!(value, E::Lit(JavaLit::Str(_) | JavaLit::StrUnits(_)))
            {
                return Ok(Kind::Bool);
            }
            return Err(self.diagnostic("NORM-SOURCE-TYPE", "String instanceof requires a compatible reference value"));
        }
        let JavaType::Class(target) = target else {
            return Err(self.diagnostic("NORM-SOURCE-TYPE", "instanceof needs a supported closed nominal type"));
        };
        if !matches!(kind, Kind::Ref | Kind::Null) ||
           !(matches!(target.as_str(), "java.lang.Object" | "Object")
               || self.program.find_class(target).is_some())
        {
            return Err(self.diagnostic("NORM-SOURCE-TYPE", "instanceof needs a supported closed nominal type and a reference value"));
        }
        if let Some(JavaType::Class(source)) = self.declared_type(value, scope) {
            if let (Some(source), Some(target)) =
                (self.program.find_class(&source), self.program.find_class(target))
            {
                if !source.is_interface() && !target.is_interface()
                    && source.full_name() != target.full_name()
                    && source.supertype().is_none() && target.supertype().is_none()
                {
                    return Err(self.diagnostic("NORM-SOURCE-TYPE", "instanceof has unrelated source classes"));
                }
            }
        }
        Ok(Kind::Bool)
    }

    fn root_object_target(&self, ty: &JavaType) -> bool {
        super::root_object_type(self.program, ty)
    }

    fn expression(&self, expr: &E, scope: &Scope) -> Result<Kind, String> {
        self.expression_in_context(expr, scope, false)
    }

    fn expression_in_context(&self, expr: &E, scope: &Scope, object_target: bool) -> Result<Kind, String> {
        let declared = || self.declared_type(expr, scope).as_ref()
            .map_or(Kind::Unresolved, Kind::declared);
        Ok(match expr {
            E::Ident(name) => {
                if !scope.contains_key(name) { self.check_receiver_use(name)?; }
                if let Some(ty) = self.declared_type(expr, scope) {
                    Kind::declared(&ty)
                } else {
                    return Err(self.diagnostic("NORM-SOURCE-NAME", &format!("undeclared value {name}")));
                }
            }
            E::Lit(JavaLit::Bool(_)) => Kind::Bool,
            E::Lit(JavaLit::Int(_) | JavaLit::Long(_) | JavaLit::Char(_)) => Kind::Number,
            E::Lit(JavaLit::Decimal { digits, radix, power, single }) => {
                if !native_literal_compiles(digits, *radix, *power, *single) {
                    return Err(self.diagnostic("NORM-NUMBER-LITERAL",
                        "nonzero Java floating literal would round to infinity or zero"));
                }
                if *single { Kind::Float } else { Kind::Double }
            }
            E::Lit(JavaLit::Str(_) | JavaLit::StrUnits(_)) => Kind::Text,
            E::Lit(JavaLit::Null) => Kind::Null,
            E::FieldAccess { target, field } => {
                if matches!(target.as_ref(), E::Ident(name) if name == "this") {
                    self.check_receiver_use(field)?;
                }
                let receiver = self.expression(target, scope)?;
                if field == "length" && receiver == Kind::Sequence {
                    return Ok(Kind::Number);
                }
                if let Some(JavaType::Class(owner)) = self.declared_type(target, scope) {
                    if let Some(declaration) = self.program.find_class(&owner) {
                        if declaration.is_record() {
                            return Err(self.diagnostic("NORM-RECORD-ACCESS",
                                "a passive record component is private; direct Java field access is not supported"));
                        }
                        if let Some(member) = declaration.fields().iter().find(|member| member.name() == field) {
                            if !super::source_member_visible(self.class, declaration, member.is_public(), member.is_private()) {
                                return Err(self.diagnostic("NORM-SOURCE-TYPE",
                                    &format!("field {owner}.{field} is not accessible from {}", self.class.full_name())));
                            }
                        }
                    }
                }
                if self.declared_type(target, scope).is_some_and(|ty|
                    super::generic_product::instantiated(self.program, &ty).is_some())
                    && self.declared_type(expr, scope).is_none()
                {
                    return Err(self.diagnostic("NORM-SOURCE-TYPE", &format!("unknown generic product field {field}")));
                }
                declared()
            }
            E::MethodCall { target, method, args } => {
                if let Some(E::Ident(receiver)) = target.as_deref() {
                    if !scope.contains_key(receiver) { self.check_receiver_use(receiver)?; }
                    if self.binding(receiver, scope).is_none() {
                        let callee = format!("{receiver}.{method}");
                        if crate::normalized_text::clock_intrinsic(self.program, self.class, &callee, args.len()) {
                            return Ok(Kind::Number);
                        }
                        use crate::normalized_decimal::{decimal_intrinsic, round_mode, DecimalIntrinsic};
                        if let Some(intrinsic) = decimal_intrinsic(self.program, self.class, &callee) {
                            let bad = || self.diagnostic("NORM-SOURCE-TYPE", &format!(
                                "{callee} requires exact decimal operands; N.round also requires an integer scale and an explicit N midpoint mode"));
                            let operands = match (intrinsic, args.as_slice()) {
                                (DecimalIntrinsic::Multiply, [_, _]) => &args[..],
                                (DecimalIntrinsic::Round, [_, scale, mode])
                                    if round_mode(mode).is_some() && self.expression(scale, scope)? == Kind::Number => &args[..1],
                                _ => return Err(bad()),
                            };
                            for operand in operands {
                                if self.expression(operand, scope)? != Kind::DecimalCarrier {
                                    return Err(bad());
                                }
                            }
                            return Ok(Kind::DecimalCarrier);
                        }
                        if crate::normalized_text::source_intrinsic(
                            self.program, self.class, &callee, args.len(),
                        ).is_some() {
                            for (index, argument) in args.iter().enumerate() {
                                let kind = self.expression(argument, scope)?;
                                if !matches!(kind, Kind::Text | Kind::Null | Kind::Unresolved)
                                    || index == 1 && matches!(kind, Kind::Null)
                                {
                                    return Err(self.diagnostic("NORM-SOURCE-TYPE",
                                        "standard text intrinsic has an argument outside its declared text domain"));
                                }
                            }
                            return Ok(Kind::Text);
                        }
                    }
                }
                if let Some(E::FieldAccess { target, field }) = target.as_deref() {
                    if matches!(target.as_ref(), E::Ident(name) if name == "this") {
                        self.check_receiver_use(field)?;
                    }
                }
                if let Some(E::FieldAccess { target, .. }) = target.as_deref() {
                    if let E::Ident(name) = target.as_ref() {
                        if name != "this" && self.program.find_class(name).is_some()
                            && self.binding(name, scope).is_some()
                        {
                            return Err(self.diagnostic("NORM-CALL-TARGET",
                                "local or receiver binding shadows the static receiver owner"));
                        }
                    }
                }
                let callee = self.method(target.as_deref(), method, args.len(), scope);
                for (i, arg) in args.iter().enumerate() {
                    let actual = self.expression(arg, scope)?;
                    if let Some(callee) = callee {
                        let expected = Kind::declared(&callee.params()[i].0);
                        if expected.is_native_decimal() || actual.is_native_decimal() {
                            self.compatible(expected, actual, &format!("argument {} of {method}", i+1))?;
                        }
                    }
                }
                declared()
            }
            E::New { java_type: ty @ JavaType::Generic(owner, _), args } => {
                let product = super::generic_product::instantiated(self.program, ty)
                    .ok_or_else(|| self.diagnostic("NORM-SOURCE-TYPE",
                        &format!("generic constructor {owner} needs explicit supported type arguments")))?;
                if args.len() != product.arity() {
                    return Err(self.diagnostic("NORM-SOURCE-TYPE", &format!("generic constructor {owner} has wrong arity")));
                }
                for arg in args {
                    let actual = self.expression(arg, scope)?;
                    if !matches!(actual, Kind::Text | Kind::Null) {
                        return Err(self.diagnostic("NORM-SOURCE-TYPE",
                            &format!("generic constructor {owner} requires the declared String type argument")));
                    }
                }
                Kind::Ref
            }
            E::New { java_type, args } => {
                let record = match java_type {
                    JavaType::Class(name) => self.program.find_class(name).filter(|class| class.is_record()),
                    _ => None,
                };
                for (index, arg) in args.iter().enumerate() {
                    let actual = self.expression(arg, scope)?;
                    if let Some(field) = record.and_then(|class| class.fields().get(index)) {
                        let label = format!("record component {}", field.name());
                        self.check_assignment_narrowing(Some(field.java_type()), arg, scope, &label)?;
                        self.compatible(Kind::declared(field.java_type()), actual, &label)?;
                    }
                }
                declared()
            }
            E::ArrayInitializer { element_type, elements } => {
                for element in elements {
                    let actual = self.expression(element, scope)?;
                    self.compatible(Kind::declared(element_type), actual, "array initializer")?;
                    self.check_assignment_narrowing(Some(element_type), element, scope, "array initializer")?;
                }
                Kind::Sequence
            }
            E::ArrayAllocation { length, .. } => {
                if self.expression(length, scope)? != Kind::Number
                    || self.integer_width(length, scope) == Some(IntegerWidth::Long)
                {
                    return Err(self.diagnostic("NORM-SOURCE-TYPE", "array dimension must have an int-compatible type"));
                }
                Kind::Sequence
            }
            E::BinOp { op, left, right } => {
                let left = self.expression(left, scope)?;
                let right = self.expression(right, scope)?;
                if left == Kind::CalendarDate || right == Kind::CalendarDate {
                    return Err(self.diagnostic("NORM-SOURCE-TYPE",
                        "the calendar-date carrier is a business value; no Java binary operation is admitted for it"));
                }
                let carrier = left == Kind::DecimalCarrier || right == Kind::DecimalCarrier;
                let text_concat = *op == B::Add && (left == Kind::Text || right == Kind::Text);
                if carrier && !text_concat
                    && matches!(op, B::Add | B::Sub | B::Mul | B::Div | B::Mod | B::Lt | B::Le | B::Gt | B::Ge)
                {
                    return Err(self.diagnostic("NORM-DECIMAL-OP",
                        "Java operators do not perform BigDecimal arithmetic; use an explicit decimal operation"));
                }
                if left.is_native_decimal() || right.is_native_decimal() {
                    let numeric = left.numeric_union(right).ok_or_else(|| self.diagnostic(
                        "NORM-SOURCE-TYPE", "native decimal operations require numeric operands",
                    ))?;
                    return match op {
                        B::Add | B::Sub | B::Mul => Ok(numeric),
                        B::Eq | B::Ne | B::Lt | B::Le | B::Gt | B::Ge => Ok(Kind::Bool),
                        _ => Err(self.diagnostic("NORM-DECIMAL-OP",
                            "当前精确业务数值入口支持加、减、乘和比较，尚无该小数运算合同")),
                    };
                }
                match op {
                    B::And | B::Or => {
                        self.condition(left, "left logical operand")?;
                        self.condition(right, "right logical operand")?;
                        Kind::Bool
                    }
                    B::Eq | B::Ne | B::Lt | B::Le | B::Gt | B::Ge => Kind::Bool,
                    B::Add if left == Kind::Text || right == Kind::Text => Kind::Text,
                    _ if left == Kind::Number && right == Kind::Number => Kind::Number,
                    _ => Kind::Unresolved,
                }
            }
            E::UnaryOp { op, operand } => {
                if matches!(*op, U::PreInc | U::PreDec | U::PostInc | U::PostDec) {
                    self.check_writable(operand, scope)?;
                }
                let kind = self.expression(operand, scope)?;
                if kind == Kind::DecimalCarrier {
                    return Err(self.diagnostic("NORM-DECIMAL-OP", "Java unary operators do not perform BigDecimal arithmetic"));
                }
                if *op == U::Not {
                    self.condition(kind, "not operand")?;
                    Kind::Bool
                } else {
                    kind
                }
            }
            E::InstanceOf { value, target, binding } => {
                if binding.is_some() {
                    return Err(self.diagnostic("NORM-SOURCE-TYPE",
                        "pattern binding requires a supported direct if condition"));
                }
                self.instanceof_type(value, target, scope)?
            }
            E::Ternary { cond, then_expr, else_expr } => {
                self.condition(self.expression(cond, scope)?, "ternary condition")?;
                let yes = self.expression_in_context(then_expr, scope, object_target)?;
                let no = self.expression_in_context(else_expr, scope, object_target)?;
                if object_target {
                    // A reference conditional in an Object assignment context
                    // can box either arm. Numeric/Boolean conditionals retain
                    // their Java category; this is not a public union value.
                    if [then_expr.as_ref(), else_expr.as_ref()].into_iter().any(|arm|
                        self.declared_type(arm, scope) == Some(JavaType::Void))
                    {
                        return Err(self.diagnostic("NORM-SOURCE-TYPE", "a conditional arm cannot return void"));
                    }
                    if yes == Kind::Bool && no == Kind::Bool { Kind::Bool }
                    else { yes.numeric_union(no).unwrap_or(Kind::Ref) }
                } else {
                    if yes.conflicts_with(no) {
                        return Err(self.diagnostic("NORM-SOURCE-TYPE", &format!(
                            "branch result type mismatch: then={} else={}", yes.label(), no.label())));
                    }
                    if yes == no { yes } else { yes.numeric_union(no).unwrap_or(Kind::Unresolved) }
                }
            }
            E::Cast { operand, .. } => {
                if self.expression(operand, scope)?.is_native_decimal() {
                    return Err(self.diagnostic("NORM-DECIMAL-OP", "尚无原生小数转整数的取整合同"));
                }
                declared()
            }
            E::ArrayAccess { target, index } => {
                if self.expression(target, scope)? != Kind::Sequence {
                    return Err(self.diagnostic("NORM-SOURCE-TYPE", "indexed receiver must be a declared array"));
                }
                if self.expression(index, scope)? != Kind::Number
                    || self.integer_width(index, scope) == Some(IntegerWidth::Long)
                {
                    return Err(self.diagnostic("NORM-SOURCE-TYPE", "array subscript must have an int-compatible type"));
                }
                declared()
            }
            _ => return Err(super::error("NORM-EXPRESSION", self.id)),
        })
    }

    fn condition(&self, kind: Kind, label: &str) -> Result<(), String> {
        if kind != Kind::Bool {
            return Err(self.diagnostic("NORM-SOURCE-TYPE", &format!("{label} requires boolean, got {}", kind.label())));
        }
        Ok(())
    }

    fn compatible(&self, expected: Kind, actual: Kind, label: &str) -> Result<(), String> {
        let narrows_native = matches!((expected, actual),
            (Kind::Number, Kind::Float | Kind::Double) | (Kind::Float, Kind::Double));
        if narrows_native || expected.conflicts_with(actual) {
            return Err(self.diagnostic("NORM-SOURCE-TYPE", &format!(
                "{label} requires {}, got {}", expected.label(), actual.label())));
        }
        Ok(())
    }

    // Source width proves assignment conversion before evaluation. It never
    // changes the normalized, exact business-number representation.
    fn integer_width(&self, expr: &E, scope: &Scope) -> Option<IntegerWidth> {
        match expr {
            E::Lit(JavaLit::Int(_) | JavaLit::Char(_)) => Some(IntegerWidth::IntFamily),
            E::Lit(JavaLit::Long(_)) => Some(IntegerWidth::Long),
            E::UnaryOp { op: U::Neg, operand } => self.integer_width(operand, scope),
            E::BinOp { op: B::Add | B::Sub | B::Mul | B::Div | B::Mod, left, right } =>
                Some(self.integer_width(left, scope)?.promote(self.integer_width(right, scope)?)),
            E::Ternary { then_expr, else_expr, .. } =>
                Some(self.integer_width(then_expr, scope)?.promote(self.integer_width(else_expr, scope)?)),
            E::Ident(_) | E::ArrayAccess { .. } | E::FieldAccess { .. }
                | E::MethodCall { .. } | E::Cast { .. } =>
                IntegerWidth::declared(&self.declared_type(expr, scope)?),
            _ => None,
        }
    }

    // Only literal expressions and their unary signs are proved here.
    // Wrapping is Java compile-time evidence, never business execution.
    fn signed_int_literal(expr: &E) -> Option<i32> {
        match expr {
            E::Lit(JavaLit::Int(value)) => Some(*value),
            E::Lit(JavaLit::Char(value)) => Some(i32::from(*value)),
            E::UnaryOp { op: U::Neg, operand } => Self::signed_int_literal(operand).map(i32::wrapping_neg),
            _ => None,
        }
    }

    fn check_assignment_narrowing(&self, expected: Option<&JavaType>, value: &E, scope: &Scope, label: &str) -> Result<(), String> {
        if expected.and_then(IntegerWidth::declared) == Some(IntegerWidth::IntFamily)
            && self.integer_width(value, scope) == Some(IntegerWidth::Long)
        {
            return Err(self.diagnostic("NORM-SOURCE-TYPE", &format!(
                "{label} cannot implicitly narrow a long expression")));
        }
        if let (Some(JavaType::Primitive(name)), Some(literal)) = (expected, Self::signed_int_literal(value)) {
            let fits = match name.as_str() {
                "byte" => i8::try_from(literal).is_ok(),
                "short" => i16::try_from(literal).is_ok(),
                "char" => u16::try_from(literal).is_ok(),
                _ => true,
            };
            if !fits {
                return Err(self.diagnostic("NORM-SOURCE-TYPE", &format!(
                    "{label}: integral literal {literal} is outside the Java {name} range")));
            }
        }
        // A parameter, mutable local or blank final is not a Java constant
        // variable. Scope excludes fields; an initialized final may be a
        // constant, so leave it to a separate constant-expression proof.
        if let (Some(JavaType::Primitive(target)), E::Ident(name)) = (expected, value) {
            if let Some(Binding { ty: JavaType::Primitive(source), initialized_final: false, .. }) = scope.get(name) {
                if matches!((source.as_str(), target.as_str()), ("int", "byte" | "short" | "char") | ("short", "byte" | "char") | ("byte", "char") | ("char", "byte" | "short")) {
                    return Err(self.diagnostic("NORM-SOURCE-TYPE", &format!(
                        "{label}: nonconstant {source} binding {name} cannot implicitly narrow to Java {target}")));
                }
            }
        }
        Ok(())
    }

    fn check_writable(&self, target: &E, scope: &Scope) -> Result<(), String> {
        if self.closed_receiver && !self.method.is_constructor() && !self.method.is_static() {
            if let Some(assembly) = self.assembly.filter(|r| !r.startup_setters.contains(self.id)) {
                if let Some(field) = super::source_ports::receiver(target)
                    .filter(|field| !matches!(target, E::Ident(_)) || !scope.contains_key(*field))
                {
                    if assembly.dependency(&self.class.full_name(), field).is_some() {
                        return Err(self.diagnostic("NORM-ENTRY-ASSEMBLY",
                            "a live receiver write requires a state contract, not its startup binding"));
                    }
                }
            }
        }
        if let E::FieldAccess { target: value, field } = target {
            if field == "length" && self.declared_type(value, scope).is_some_and(|ty| matches!(ty, JavaType::Array(_))) {
                return Err(self.diagnostic("NORM-SOURCE-TYPE", "cannot update final array length"));
            }
            if self.declared_type(value, scope).is_some_and(|ty|
                super::generic_product::instantiated(self.program, &ty).is_some())
            {
                return Err(self.diagnostic("NORM-SOURCE-TYPE", &format!("cannot update final generic product field {field}")));
            }
            if let Some(JavaType::Class(owner)) = self.declared_type(value, scope) {
                if let Some(declaration) = self.program.find_class(&owner) {
                    let in_own_constructor = self.method.is_constructor()
                        && declaration.full_name() == self.class.full_name()
                        && matches!(value.as_ref(), E::Ident(name) if name == "this");
                    if !in_own_constructor {
                        if declaration.fields().iter().any(|f| f.name() == field && super::source_ports::marked(declaration, f)) {
                            return Err(self.diagnostic("NORM-EXTERNAL-FIELD",
                                &format!("external receiver {field} cannot be reassigned after its certified assembly")));
                        }
                        if declaration.fields().iter().any(|f| f.name() == field && f.is_final()) {
                            return Err(self.diagnostic("NORM-SOURCE-TYPE", &format!("cannot update final field {field}")));
                        }
                    }
                }
            }
        }
        if let E::Ident(name) = target {
            if !scope.contains_key(name) && !self.method.is_constructor()
                && self.class.fields().iter().any(|field| field.name() == name && super::source_ports::marked(self.class, field))
            {
                return Err(self.diagnostic("NORM-EXTERNAL-FIELD",
                    &format!("external receiver {name} cannot be reassigned after its certified assembly")));
            }
            if scope.get(name).is_some_and(|binding| binding.initialized_final) {
                return Err(self.diagnostic("NORM-SOURCE-TYPE", &format!(
                    "cannot update final local {name}")));
            }
        }
        Ok(())
    }

    fn assignment(&self, target: &E, value: &E, is_compound: bool, scope: &Scope) -> Result<(), String> {
        self.check_writable(target, scope)?;
        let expected = self.expression(target, scope)?;
        let object_target = !is_compound && self.declared_type(target, scope).as_ref()
            .is_some_and(|ty| self.root_object_target(ty));
        let actual = self.expression_in_context(value, scope, object_target)?;
        if let Some(target_ty) = self.declared_type(target, scope) {
            if super::generic_product::instantiated(self.program, &target_ty).is_some()
                && !self.declared_type(value, scope).is_some_and(|actual|
                    super::generic_product::same_instantiation(self.program, &target_ty, &actual))
            {
                return Err(self.diagnostic("NORM-SOURCE-TYPE", "generic product assignment has different type arguments"));
            }
        }
        let label = match target {
            E::Ident(name) => format!("assignment to {name}"),
            E::FieldAccess { field, .. } => format!("assignment to {field}"),
            _ => "assignment".into(),
        };
        if !is_compound {
            self.check_assignment_narrowing(self.declared_type(target, scope).as_ref(), value, scope, &label)?;
        }
        self.compatible(expected, actual, &label)
    }

    // Child scopes start with the currently visible parameters/locals.
    // Fields are resolved separately and may be hidden by a local name.
    fn check_local_name(&self, name: &str, scope: &Scope) -> Result<(), String> {
        if scope.contains_key(name) {
            return Err(self.diagnostic("NORM-SOURCE-NAME", &format!(
                "duplicate local declaration {name} overlaps a visible binding")));
        }
        Ok(())
    }

    fn prove_initialized_counter(&self, header: &super::fixed_for::FixedFor<'_>, body: &[S], scope: &Scope) -> Result<(), String> {
        let name = header.counter;
        let not_proven = |detail: &str| crate::preprocess::required(
            "NORM-LOOP", self.id, detail,
            "保留合法源码；对于固定循环，计数器必须在可证范围内单向前进。",
        );
        let inspected = scope.get(name).filter(|binding| binding.initialized_local)
            .and_then(|binding| match &binding.ty {
                JavaType::Primitive(ty) => Some(ty.as_str()), _ => None,
            });
        let narrow = match inspected {
            Some("int" | "long") => false,
            Some("short") => {
                if i16::try_from(header.start).is_err()
                    || i16::try_from(header.start + header.count * header.step).is_err()
                {
                    return Err(not_proven("short loop counter may overflow before its final update"));
                }
                true
            }
            _ => return Err(not_proven("assignment counter needs an initialized integer local")),
        };
        if !super::fixed_for::closed_assignment_counter(body, name, !narrow) {
            return Err(not_proven(&format!("unproved write to induction counter {name}")));
        }
        Ok(())
    }

    fn block(&self, body: &[S], scope: &mut Scope) -> Result<(), String> {
        for statement in body {
            match statement {
                S::VarDecl { java_type, name, initializer, is_final } => {
                    self.check_local_name(name, scope)?;
                    if let Some(value) = initializer {
                        if super::host_collection_allocation(java_type, value) {
                            scope.insert(name.clone(), Binding::plain(java_type.clone()));
                            continue;
                        }
                        let actual = self.expression_in_context(value, scope, self.root_object_target(java_type))?;
                        if super::generic_product::instantiated(self.program, java_type).is_some()
                            && !self.declared_type(value, scope).is_some_and(|actual|
                                super::generic_product::same_instantiation(self.program, java_type, &actual))
                        {
                            return Err(self.diagnostic("NORM-SOURCE-TYPE", "generic product initializer has different type arguments"));
                        }
                        let label = format!("initializer for {name}");
                        // Structural admission already required a local name.
                        // Record arrays share one sequence type, so resolve
                        // that name and keep its exact Java component identity.
                        if let Some(component) = super::record_array_component(java_type) {
                            if !super::declared_reference(component, self.layouts)
                                || self.declared_type(value, scope).as_ref() != Some(java_type)
                            {
                                return Err(self.diagnostic("NORM-SOURCE-TYPE", &format!(
                                    "{label} must alias an array of the same declared record component")));
                            }
                        }
                        self.check_assignment_narrowing(Some(java_type), value, scope, &label)?;
                        self.compatible(Kind::declared(java_type), actual, &label)?;
                    }
                    scope.insert(name.clone(), Binding {
                        ty: java_type.clone(),
                        initialized_final: *is_final && initializer.is_some(),
                        initialized_local: initializer.is_some(),
                    });
                }
                S::ThisConstructor(args) => {
                    let only_parameter_value = |name: &str| self.method.params().iter().any(|(_, n)| n == name);
                    fn pure_arg(value: &E, has_parameter: &impl Fn(&str) -> bool) -> bool {
                        match value {
                            E::Lit(_) => true,
                            E::Ident(name) => has_parameter(name),
                            E::BinOp { left, right, .. } =>
                                pure_arg(left, has_parameter) && pure_arg(right, has_parameter),
                            E::UnaryOp { op: U::Neg | U::Not, operand } =>
                                pure_arg(operand, has_parameter),
                            E::Cast { operand, .. } => pure_arg(operand, has_parameter),
                            _ => false,
                        }
                    }
                    let (_, constructor) = super::constructor_for_arity(self.class, args.len())
                        .ok_or_else(|| self.diagnostic("NORM-UNBOUND-CALL", "unresolved same-class constructor"))?;
                    for ((expected, _), arg) in constructor.params().iter().zip(args) {
                        if !pure_arg(arg, &only_parameter_value) {
                            return Err(self.diagnostic("NORM-SOURCE-TYPE",
                                "same-class delegation must not observe the receiver before construction"));
                        }
                        let actual = self.expression_in_context(arg, scope, self.root_object_target(expected))?;
                        self.check_assignment_narrowing(Some(expected), arg, scope, "constructor argument")?;
                        self.compatible(Kind::declared(expected), actual, "constructor argument")?;
                    }
                }
                S::Assign { target, value, is_compound } => {
                    self.assignment(target, value, *is_compound, scope)?;
                }
                S::Return(Some(value)) => {
                    let return_type = self.method.return_type();
                    if matches!(return_type, JavaType::Void) {
                        return Err(self.diagnostic("NORM-SOURCE-TYPE", "void completion cannot return a value"));
                    }
                    let actual = self.expression_in_context(value, scope, self.root_object_target(return_type))?;
                    self.check_assignment_narrowing(Some(return_type), value, scope, "return")?;
                    self.compatible(Kind::declared(return_type), actual, "return")?;
                }
                S::Return(None) => {
                    if !matches!(self.method.return_type(), JavaType::Void) {
                        return Err(self.diagnostic("NORM-SOURCE-TYPE", "a value-returning method cannot use bare return"));
                    }
                }
                S::If { cond, then_block, else_block } => {
                    let mut yes = scope.clone();
                    let mut no = scope.clone();
                    if let Some(pattern) = cond.direct_type_pattern() {
                        if !super::platform_string_type(self.program, pattern.target) {
                            return Err(self.diagnostic("NORM-SOURCE-TYPE", "pattern target requires a supported reference narrowing"));
                        }
                        self.instanceof_type(pattern.operand, pattern.target, scope)?;
                        self.check_local_name(pattern.name, scope)?;
                        let binding = Binding {
                            ty: JavaType::Class("java.lang.String".into()),
                            initialized_final: false,
                            initialized_local: true,
                        };
                        let matched = if pattern.matched_when_true { &mut yes } else { &mut no };
                        matched.insert(pattern.name.to_owned(), binding.clone());
                        if S::pattern_in_continuation(pattern.matched_when_true, then_block, else_block.as_deref()) {
                            scope.insert(pattern.name.to_owned(), binding);
                        }
                    } else {
                        self.condition(self.expression(cond, scope)?, "if condition")?;
                    }
                    self.block(then_block, &mut yes)?;
                    if let Some(body) = else_block { self.block(body, &mut no)?; }
                }
                S::Switch { selector, cases, default } => {
                    let actual = self.expression(selector, scope)?;
                    if actual != Kind::Number || self.integer_width(selector, scope) != Some(IntegerWidth::IntFamily) {
                        return Err(self.diagnostic("NORM-SOURCE-TYPE",
                            "switch selector requires a proven byte, short, char or int value"));
                    }
                    for (label, body) in cases {
                        let target = self.declared_type(selector, scope);
                        self.check_assignment_narrowing(target.as_ref(), label, scope, "switch label")?;
                        if let Some(JavaType::Primitive(name)) = target {
                            let representable = Self::signed_int_literal(label).is_some_and(|value| match name.as_str() {
                                "byte" => i8::try_from(value).is_ok(),
                                "short" => i16::try_from(value).is_ok(),
                                "char" => u16::try_from(value).is_ok(),
                                _ => true,
                            });
                            if !representable {
                                return Err(self.diagnostic("NORM-SOURCE-TYPE", &format!("switch label is outside the Java {name} range")));
                            }
                        }
                        self.block(body, &mut scope.clone())?;
                    }
                    self.block(default, &mut scope.clone())?;
                }
                S::Try { resource, body, catches, finally_block } => {
                    let mut body_scope = scope.clone();
                    if let Some(resource) = resource {
                        self.block(std::slice::from_ref(resource.as_ref()), &mut body_scope)?;
                    }
                    self.block(body, &mut body_scope)?;
                    for handler in catches {
                        let mut handler_scope = scope.clone();
                        self.check_local_name(handler.var_name(), scope)?;
                        let mut binding = Binding::plain(JavaType::Class(handler.exception_classes()[0].clone()));
                        // The handler shares all alternatives. The interpreter
                        // models type-only exception objects; in particular a
                        // multi-catch binding is implicitly final in Java.
                        binding.initialized_final = handler.exception_classes().len() > 1;
                        handler_scope.insert(handler.var_name().into(), binding);
                        self.block(handler.body(), &mut handler_scope)?;
                    }
                    if let Some(body) = finally_block { self.block(body, &mut scope.clone())?; }
                }
                S::Block(body) => self.block(body, &mut scope.clone())?,
                S::Throw(value) | S::ExprStmt(value) => { self.expression(value, scope)?; }
                // Structural admission already required both components of
                // each fixed or rectangular loop. Only check their bindings here.
                S::For { init: Some(init), cond, update: Some(update), body } => {
                    let mut loop_scope = scope.clone();
                    let assignment = super::fixed_for::assignment_init(init.as_ref());
                    if let Some(header) = super::fixed_for::FixedFieldFor::from_for(statement) {
                        self.assignment(header.initial_target, header.initial_value, false, scope)?;
                    } else if let Some((_, target, value)) = assignment {
                        self.assignment(target, value, false, scope)?;
                    } else if let S::Block(declarations) = init.as_ref() {
                        self.block(declarations, &mut loop_scope)?;
                    } else {
                        self.block(std::slice::from_ref(init.as_ref()), &mut loop_scope)?;
                    }
                    if super::fixed_for::BoundedControlFor::from_for(statement).is_some() {
                        let condition = cond.as_ref().expect("closed induction condition");
                        self.condition(self.expression(condition, &loop_scope)?, "for condition")?;
                    }
                    self.expression(update, &loop_scope)?;
                    self.block(body, &mut loop_scope)?;
                    if assignment.is_some() {
                        let header = super::fixed_for::FixedFor::from_header(Some(init), cond.as_ref(), Some(update))
                            .expect("structural admission proved the fixed assignment counter");
                        self.prove_initialized_counter(&header, body, scope)?;
                    }
                }
                S::ForEach { var_type, var_name, iterable, body } => {
                    self.expression(iterable, scope)?;
                    self.check_local_name(var_name, scope)?;
                    let mut loop_scope = scope.clone();
                    loop_scope.insert(var_name.clone(), Binding::plain(var_type.clone()));
                    self.block(body, &mut loop_scope)?;
                }
                // Structural admission rejects raw while and incomplete for
                // headers; directives and empty returns have no bindings here.
                S::LocalIr(_) if self.class.fields().iter().any(|f| self.unmodeled_receiver_field(f)) =>
                    return Err(self.diagnostic("NORM-ENTRY-STATE",
                        "cannot prove plain receiver fields are unused by local semantic directives")),
                S::LoopBreak | S::LoopContinue => {}
                S::While { .. } | S::For { .. } | S::LocalIr(_) => {}
            }
        }
        Ok(())
    }
}

pub(super) fn validate(program: &JavaProgram, class: &JavaClass, method: &JavaMethod, id: &str, body: &[S], layouts: &BTreeMap<String, BTreeMap<String, String>>, closed_receiver: bool, assembly: Option<&crate::kernel_input::SourceReceiver>) -> Result<(), String> {
    let mut scope = method.params().iter().map(|(ty, name)| (name.clone(), Binding::plain(ty.clone()))).collect();
    Admission { program, class, method, id, layouts, closed_receiver, assembly }.block(body, &mut scope)
}
