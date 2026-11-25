//! # math
//!
//! 纯数学演算模块。Expr 代数 + normalize 化简 + Primitive Functions 注册表。
//!
//! ## 设计原则
//!
//! - **语言无关**：math 不认识 null/NPE/heap/generics/Java。它只认识符号、函数、代数规则。
//! - **构造函数守门**：所有 Expr 必须通过 builder 工厂函数构造，私有字段杜绝裸构造。
//! - **Primitive Function 身份凭证**：Call 必须持有 FuncKey，FuncKey 只能从 `register_func()` 拿。
//! - **normalize 单一入口**：`normalize(expr) → expr` 是化简唯一对外 API，内部 rebuild-with-children。
//!
//! ## 公开 API 面
//!
//! - `Expr` — 表达式类型（不透明）
//! - `SymVarOrigin` — 符号变量来源标签（Parameter / External / Temporal）
//! - `FuncKey` — Primitive Function 身份凭证（只能从 register_func 获得）
//! - `FuncSemantics` — Primitive Function 语义声明（纯函数、互补、恒等等）
//! - `builder::*` — Expr 工厂函数
//! - `register_func()` — Func 注册（返回 FuncKey）
//! - `normalize()` — 化简入口
//!
//! 凡未通过上述 API 构造的 Expr，编译器拒绝。

pub mod builder;
mod cache;
pub mod case_orientation;
pub mod domains;
pub mod expr;
pub mod normalize;
pub mod canonical_order;
pub mod primitive;
pub mod primitive_enum;
mod profile;
pub mod sort_context;
pub mod symvar;

pub use domains::primitive::registry::{
    func_arity, func_name, func_semantics, register_func,
    FuncKey, FuncSemantics,
};
pub use expr::{BinOpKind, Expr, Lit, RelationAggregateKind, UnaryOpKind};
pub use normalize::{
    normalize, normalize_with, try_normalize_with, BudgetKind, NormalizationLimit,
};
pub use canonical_order::{
    canonical_order_compare, canonical_order_key, canonical_order_ordered_comparison,
};
pub use primitive::{is_any_primitive, is_java_primitive};
pub use primitive_enum::Primitive;
pub use sort_context::{DefaultSortContext, SortContext};
pub use symvar::SymVarOrigin;

/// Exact mathematical integer, distinct from source-language machine words.
pub use num_bigint::BigInt as Integer;
