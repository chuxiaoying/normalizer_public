//! # simulator-parser
//!
//! Java 源码 → AST。这是 simulator 的第一阶段：纯词法与语法分析。
//!
//! ## 设计原则
//!
//! - **纯语法**：不做任何语义推断（不推类型、不解析重载、不识别框架注解）。
//!   语义推断是 resolver 的职责
//! - **产出不透明**：`JavaProgram` / `JavaClass` / `JavaMethod` 字段私有，
//!   外部只能通过 pub API 查询，不能直接构造/修改
//! - **Lombok 等代码生成**：由引擎外的 AI 预处理准备明确源码；不在解析后执行插件
//!
//! ## 公开 API
//!
//! - `parse(source, opts) → Result<JavaProgram>`
//! - `JavaProgram::classes() → impl Iterator<Item=&JavaClass>`
//! - `JavaClass` / `JavaMethod` / `JavaStatement` / `JavaExpr` 的只读访问器
//!
//! ## 不做的事
//!
//! - 不做类型推断（Var<T>、泛型、重载）
//! - 不解析框架注解含义（@Autowired、@Bean 的含义由预处理明确）
//! - 不读文件（由 pipeline 读取后送入字符串）

pub mod ast;
pub mod parser;

pub use ast::{
    AnnotationRef, CatchClause, JavaBinOp, JavaClass, JavaExpr, JavaField, JavaLit, JavaMethod,
    JavaProgram, JavaStatement, JavaType, JavaUnaryOp,
};
pub use parser::{is_reserved_identifier, parse, ParseError, ParseOptions};
pub(crate) mod body_parser;
