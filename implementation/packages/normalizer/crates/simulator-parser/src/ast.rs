//! Java AST —— 语法层面的数据结构。
//!
//! 设计原则：
//! - 所有类型字段 pub(crate)，外部通过访问器查询
//! - 语法节点由 parser 构造，不暴露外部 AST 改写入口
//! - 无伪函数名（`__field__`/`__pfield__` 等 legacy 残留已清零）
//! - BinOp/UnaryOp 强类型枚举，编译期防打错 + 模式穷尽
//! - parser 不做类型推断（权责交给 resolver）

pub const NORMALIZER_INTRINSICS_IMPORT: &str = "normalizer.intrinsics.N";

/// Java 程序：一组类定义的集合。
#[derive(Debug, Clone)]
pub struct JavaProgram {
    pub(crate) classes: Vec<JavaClass>,
}

impl JavaProgram {
    /// 构造空程序。pipeline 层用此作为合并起点。
    pub fn new() -> Self {
        Self {
            classes: Vec::new(),
        }
    }

    pub fn classes(&self) -> &[JavaClass] {
        &self.classes
    }

    pub fn find_class(&self, full_name: &str) -> Option<&JavaClass> {
        self.classes.iter().find(|c| c.full_name() == full_name)
    }

    /// 追加一个类。pipeline 层多文件合并时使用。
    /// 不做重复检测——同名 class 会同时存在，由调用方负责语义去重。
    pub fn push_class(&mut self, class: JavaClass) {
        self.classes.push(class);
    }
}

/// Java 类。
#[derive(Debug, Clone)]
pub struct JavaClass {
    pub(crate) source_file: String,
    pub(crate) package: String,
    pub(crate) is_public: bool,
    /// 显式 import 的完整限定名列表（`import x.y.Z;`）
    ///
    /// parser 原始输出，不做 FQN 化决策——FQN 化由 resolver 在类型推断收口点完成。
    pub(crate) imports: Vec<String>,
    /// Static and wildcard imports are syntactically valid, but unresolved
    /// bindings cannot be admitted to the closed source contract.
    pub(crate) has_unbound_imports: bool,
    pub(crate) has_unknown_import_origins: bool,
    pub(crate) simple_name: String,
    pub(crate) type_parameters: Vec<String>,
    pub(crate) annotations: Vec<AnnotationRef>,
    pub(crate) fields: Vec<JavaField>,
    pub(crate) static_initializers: Vec<(usize, Vec<JavaStatement>)>,
    pub(crate) methods: Vec<JavaMethod>,
    pub(crate) inner_classes: Vec<JavaClass>,
    pub(crate) supertype: Option<String>,
    pub(crate) interfaces: Vec<String>,
    pub(crate) interface_arguments: std::collections::BTreeMap<String, Vec<JavaType>>,
    pub(crate) is_interface: bool,
    pub(crate) is_record: bool,
}

impl JavaClass {

    pub fn type_parameters(&self) -> &[String] { &self.type_parameters }

    /// Origin supplied when this declaration was parsed.
    pub fn source_file(&self) -> &str {
        &self.source_file
    }

    pub fn imports(&self) -> &[String] {
        &self.imports
    }

    /// Does the simple name N have the one supported explicit import?
    /// Project-wide source declarations are checked by source admission.
    pub fn imports_normalizer_intrinsics(&self) -> bool {
        let mut candidates = self.imports.iter().filter(|name| name.rsplit('.').next() == Some("N"));
        !self.has_unbound_imports
            && candidates.next().is_some_and(|name| name == NORMALIZER_INTRINSICS_IMPORT)
            && candidates.all(|name| name == NORMALIZER_INTRINSICS_IMPORT)
    }
    pub fn has_unknown_import_origins(&self) -> bool { self.has_unknown_import_origins }

    pub fn has_unbound_imports(&self) -> bool {
        self.has_unbound_imports
    }
    pub fn simple_name(&self) -> &str {
        &self.simple_name
    }
    pub fn package(&self) -> &str {
        &self.package
    }

    pub fn is_public_top_level(&self) -> bool {
        self.is_public && !self.simple_name.contains('.')
    }

    /// A zero-argument constructor that adds no behavior to ordinary
    /// default construction. For a named constructor this proof deliberately
    /// requires public visibility; its caller need not guess access rights.
    pub fn has_trivial_noarg_constructor(&self) -> bool {
        let mut declared = self.methods.iter().filter(|method| method.is_constructor).peekable();
        if declared.peek().is_none() {
            return !self.is_record || self.fields.is_empty();
        }
        let mut empty_signature = declared.filter(|method| method.params.is_empty());
        empty_signature.next().is_some_and(|method| {
            method.is_public && !method.is_private && method.throws.is_empty()
                && method.annotations.is_empty() && method.body.as_ref().is_some_and(Vec::is_empty)
        }) && empty_signature.next().is_none()
    }

    pub fn full_name(&self) -> String {
        if self.package.is_empty() { self.simple_name.clone() }
        else { format!("{}.{}", self.package, self.simple_name) }
    }
    pub fn annotations(&self) -> &[AnnotationRef] {
        &self.annotations
    }
    pub fn fields(&self) -> &[JavaField] {
        &self.fields
    }
    /// Each block is paired with the number of fields preceding it in source.
    pub fn static_initializers(&self) -> &[(usize, Vec<JavaStatement>)] {
        &self.static_initializers
    }
    pub fn methods(&self) -> &[JavaMethod] {
        &self.methods
    }
    pub fn take_inner_classes(&mut self) -> Vec<JavaClass> {
        std::mem::take(&mut self.inner_classes)
    }
    pub fn supertype(&self) -> Option<&str> {
        self.supertype.as_deref()
    }
    pub fn interface_arguments(&self, name: &str) -> &[JavaType] {
        self.interface_arguments.get(name).map(Vec::as_slice).unwrap_or_default()
    }
    pub fn interfaces(&self) -> &[String] {
        &self.interfaces
    }
    /// A native Java record header supplies immutable positional components.
    pub fn is_record(&self) -> bool {
        self.is_record
    }
    pub fn is_interface(&self) -> bool {
        self.is_interface
    }
}

/// Java 字段。
#[derive(Debug, Clone)]
pub struct JavaField {
    pub(crate) name: String,
    pub(crate) java_type: JavaType,
    pub(crate) is_static: bool,
    pub(crate) is_final: bool,
    pub(crate) is_public: bool,
    pub(crate) is_private: bool,
    pub(crate) annotations: Vec<AnnotationRef>,
    pub(crate) initializer: Option<JavaExpr>,
}

impl JavaField {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn java_type(&self) -> &JavaType {
        &self.java_type
    }
    pub fn is_static(&self) -> bool {
        self.is_static
    }
    pub fn is_final(&self) -> bool {
        self.is_final
    }
    pub fn is_public(&self) -> bool {
        self.is_public
    }
    pub fn is_private(&self) -> bool {
        self.is_private
    }
    pub fn annotations(&self) -> &[AnnotationRef] {
        &self.annotations
    }
    pub fn initializer(&self) -> Option<&JavaExpr> {
        self.initializer.as_ref()
    }
    pub fn has_initializer(&self) -> bool {
        self.initializer.is_some()
    }
}

/// Java 方法。
///
/// `body` 为 `None` 表示没有 Java 方法体。
#[derive(Debug, Clone)]
pub struct JavaMethod {
    pub(crate) type_parameters: Vec<String>,
    pub(crate) name: String,
    pub(crate) params: Vec<(JavaType, String)>,
    pub(crate) param_annotations: std::collections::BTreeMap<String, Vec<AnnotationRef>>,
    pub(crate) throws: Vec<String>,
    pub(crate) return_type: JavaType,
    pub(crate) is_static: bool,
    pub(crate) is_constructor: bool,
    pub(crate) is_default: bool,
    pub(crate) is_public: bool,
    pub(crate) is_private: bool,
    pub(crate) is_native: bool,
    pub(crate) annotations: Vec<AnnotationRef>,
    pub(crate) body: Option<Vec<JavaStatement>>,
}

impl JavaMethod {
    pub fn type_parameters(&self) -> &[String] { &self.type_parameters }

    pub fn is_native(&self) -> bool { self.is_native }
    pub fn is_default(&self) -> bool { self.is_default }
    pub fn is_local_lambda_helper(&self) -> bool { self.name.starts_with("<lambda:") }

    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn param_annotations(&self, name: &str) -> &[AnnotationRef] {
        self.param_annotations.get(name).map(Vec::as_slice).unwrap_or_default()
    }
    pub fn params(&self) -> &[(JavaType, String)] {
        &self.params
    }
    pub fn throws(&self) -> &[String] {
        &self.throws
    }
    pub fn return_type(&self) -> &JavaType {
        &self.return_type
    }
    pub fn is_static(&self) -> bool {
        self.is_static
    }
    pub fn is_public(&self) -> bool { self.is_public }
    pub fn is_private(&self) -> bool { self.is_private }

    pub fn is_constructor(&self) -> bool {
        self.is_constructor
    }
    pub fn annotations(&self) -> &[AnnotationRef] {
        &self.annotations
    }
    pub fn body(&self) -> Option<&[JavaStatement]> {
        self.body.as_deref()
    }
    pub fn arity(&self) -> usize {
        self.params.len()
    }
}

/// Java 类型（语法层面的类型表示）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JavaType {
    Primitive(String),
    Class(String),
    Array(Box<JavaType>),
    Generic(String, Vec<JavaType>),
    Void,
}

/// Source annotation marker; only the name and presence of arguments matter.
#[derive(Debug, Clone)]
pub struct AnnotationRef {
    pub(crate) name: String,
    pub(crate) has_arguments: bool,
}

impl AnnotationRef {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn has_arguments(&self) -> bool {
        self.has_arguments
    }
}

// ============================================================
// 语句 AST
// ============================================================

/// Java 语句。
#[derive(Debug, Clone)]
pub enum JavaStatement {
    /// Explicit semantics supplied by preprocessing at this statement position.
    LocalIr(normalizer_ir::local::LocalIr),
    /// 变量声明：`int x = 1;` 或 `Foo f;`（无 init）
    VarDecl {
        java_type: JavaType,
        name: String,
        initializer: Option<JavaExpr>,
        is_final: bool,
    },

    /// 赋值：`x = expr;` / `obj.field = expr;` / `arr[i] = expr;`
    Assign {
        target: JavaExpr,
        value: JavaExpr,
        is_compound: bool,
    },

    /// Explicit `this(...)` at the head of a constructor.
    ThisConstructor(Vec<JavaExpr>),

    /// 纯表达式语句：`foo();` / `x++;`
    ExprStmt(JavaExpr),

    /// return 语句：`return` 或 `return expr;`
    Return(Option<JavaExpr>),

    /// if 语句：`if (c) {...} else {...}`
    If {
        cond: JavaExpr,
        then_block: Vec<JavaStatement>,
        else_block: Option<Vec<JavaStatement>>,
    },

    /// A native bounded switch with one dispatch expression and closed arms.
    Switch {
        selector: JavaExpr,
        cases: Vec<(Vec<JavaExpr>, Vec<JavaStatement>)>,
        default: Vec<JavaStatement>,
        // For actual colon fall-through, preserve each source group and
        // the default's lexical position. Independent switches need none.
        fall_through: Option<SwitchFallThrough>,
    },

    /// while 循环
    While {
        cond: JavaExpr,
        body: Vec<JavaStatement>,
    },

    /// for 循环（3 段式）
    For {
        init: Option<Box<JavaStatement>>,
        cond: Option<JavaExpr>,
        update: Option<JavaExpr>,
        body: Vec<JavaStatement>,
    },

    /// for-each: `for (T x : iterable) body`
    ForEach {
        var_type: JavaType,
        var_name: String,
        iterable: JavaExpr,
        body: Vec<JavaStatement>,
    },

    /// try-catch-finally
    Try {
        resource: Option<Box<JavaStatement>>,
        body: Vec<JavaStatement>,
        catches: Vec<CatchClause>,
        finally_block: Option<Vec<JavaStatement>>,
    },

    /// throw 语句
    Throw(JavaExpr),

    /// Unlabeled native abrupt transfer. Only a certified enclosing loop
    /// may consume it; a switch's own separating break is parsed earlier.
    LoopBreak,
    LoopContinue,

    /// 代码块
    Block(Vec<JavaStatement>),
}

impl JavaStatement {
    /// Establish post-if visibility only when the native statement forms
    /// prove that the unmatched arm cannot fall through and the matched one
    /// can. This is lexical Java reachability, not symbolic feasibility.
    pub fn pattern_in_continuation(
        matched_when_true: bool, then_block: &[Self], else_block: Option<&[Self]>,
    ) -> bool {
        fn block_completion(body: &[JavaStatement]) -> Option<bool> {
            use JavaStatement::*;
            match body.last() {
                None | Some(VarDecl { .. } | Assign { .. } | ExprStmt(_) | LocalIr(_)) => Some(true),
                Some(Return(_) | Throw(_)) => Some(false),
                Some(Block(inner)) => block_completion(inner),
                Some(If { else_block: None, .. }) => Some(true),
                Some(If { then_block, else_block: Some(other), .. }) => {
                    match (block_completion(then_block), block_completion(other)) {
                        (Some(true), _) | (_, Some(true)) => Some(true),
                        (Some(false), Some(false)) => Some(false),
                        _ => None,
                    }
                }
                _ => None,
            }
        }
        let yes = block_completion(then_block);
        let no = block_completion(else_block.unwrap_or_default());
        if matched_when_true { yes == Some(true) && no == Some(false) }
        else { no == Some(true) && yes == Some(false) }
    }
}

/// catch 子句。
#[derive(Debug, Clone)]
pub struct CatchClause {
    pub(crate) exception_classes: Vec<String>,
    pub(crate) var_name: String,
    pub(crate) body: Vec<JavaStatement>,
}

impl CatchClause {
    pub fn exception_classes(&self) -> &[String] {
        &self.exception_classes
    }
    pub fn var_name(&self) -> &str {
        &self.var_name
    }
    pub fn body(&self) -> &[JavaStatement] {
        &self.body
    }
}

// ============================================================
// 表达式 AST
// ============================================================

/// Java 表达式。
#[derive(Debug, Clone)]
pub enum JavaExpr {
    /// Source-only lambda syntax; the linker must certify it before execution.
    LocalLambda { parameters: Vec<String>, value: Box<JavaExpr> },
    /// 字面量
    Lit(JavaLit),

    /// 标识符引用（变量名/类名/字段名，由 resolver 决定）
    Ident(String),

    /// Native Java type test, optionally introducing a pattern variable.
    InstanceOf { value: Box<JavaExpr>, target: JavaType, binding: Option<String> },

    /// 字段访问：`obj.field`
    FieldAccess {
        target: Box<JavaExpr>,
        field: String,
    },

    /// 方法调用：`target.method(args)` 或 `method(args)`（target=None）
    MethodCall {
        target: Option<Box<JavaExpr>>,
        method: String,
        args: Vec<JavaExpr>,
    },

    /// `new ClassName(args)`
    New {
        java_type: JavaType,
        args: Vec<JavaExpr>,
    },

    /// `new T[]{...}` or the declaration-only shorthand `{...}`.
    ArrayInitializer {
        element_type: JavaType,
        elements: Vec<JavaExpr>,
    },

    /// `new T[length]`: unlike an initializer, every element starts at its default.
    ArrayAllocation {
        element_type: JavaType,
        length: Box<JavaExpr>,
    },

    /// 二元运算
    BinOp {
        op: JavaBinOp,
        left: Box<JavaExpr>,
        right: Box<JavaExpr>,
    },

    /// 赋值表达式：`(x = expr)`。
    ///
    /// Java 语义：赋值既会写入 target，也会返回右值；因此它可以出现在
    /// for/while 条件、方法参数、二元表达式子树等表达式位置。
    Assign {
        target: Box<JavaExpr>,
        value: Box<JavaExpr>,
        // The value is lowered to a binary expression for compound assignments.
        // Source admission still needs the original assignment conversion.
        is_compound: bool,
    },

    /// 一元运算。
    ///
    /// 注意：PostInc/PostDec/PreInc/PreDec 的符号执行语义需 executor 模拟"读→存→增"三步。
    UnaryOp {
        op: JavaUnaryOp,
        operand: Box<JavaExpr>,
    },

    /// 三元运算：`cond ? a : b`
    Ternary {
        cond: Box<JavaExpr>,
        then_expr: Box<JavaExpr>,
        else_expr: Box<JavaExpr>,
    },

    /// 类型转换：`(T) expr`
    ///
    /// 注意：Cast 有两种语义，executor 实现时分路处理：
    /// - 基本类型转换（`(int) 5.5`）—— pure 求值
    /// - 引用类型转换（`(String) obj`）—— 可能 ClassCastException，需 Temporal 封印
    Cast {
        target_type: JavaType,
        operand: Box<JavaExpr>,
    },

    /// 数组元素访问：`a[i]`
    ArrayAccess {
        target: Box<JavaExpr>,
        index: Box<JavaExpr>,
    },
}

#[derive(Debug, Clone)]
pub struct SwitchFallThrough {
    pub default_position: Option<usize>,
    pub continues: Vec<bool>,
}

/// A type-pattern condition and the Boolean branch where it binds.
pub struct JavaTypePattern<'a> {
    pub operand: &'a JavaExpr,
    pub target: &'a JavaType,
    pub name: &'a str,
    pub matched_when_true: bool,
}

impl JavaExpr {
    pub fn direct_type_pattern(&self) -> Option<JavaTypePattern<'_>> {
        match self {
            Self::InstanceOf { value, target, binding: Some(name) } => Some(JavaTypePattern {
                operand: value, target, name, matched_when_true: true,
            }),
            Self::UnaryOp { op: JavaUnaryOp::Not, operand } => {
                let mut pattern = operand.direct_type_pattern()?;
                pattern.matched_when_true = !pattern.matched_when_true;
                Some(pattern)
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum JavaLit {
    Int(i32),
    Long(i64),
    // Digits are in radix 10 or 16; the power is of 10 or 2 respectively.
    Decimal { digits: String, radix: u32, power: i32, single: bool },
    Bool(bool),
    Str(String),
    StrUnits(Vec<u16>),
    Char(u16),
    Null,
}

/// Java 二元运算符。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JavaBinOp {
    // 算术
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    // 比较
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    // 逻辑
    And,
    Or,
    // 位运算
}

/// Java 一元运算符。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JavaUnaryOp {
    Neg,     // -x
    Not,     // !x
    PreInc,  // ++x
    PreDec,  // --x
    PostInc, // x++
    PostDec, // x--
}
