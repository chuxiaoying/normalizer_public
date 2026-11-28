//! Typed mathematical expressions and normalization for already acquired values.
//!
//! Host execution owns source evaluation, heap mutation, exceptions and
//! observations. This crate compares semantic value structure and applies
//! math laws within the binding and path evidence provided by the caller.
//! A null literal or an identity token does not execute host behavior.
//!
//! Use `builder` for expression construction, the primitive registry for
//! checked function identities, and `normalize` or `try_normalize_with` for
//! computation. Supply `SortContext` when the value refers to acquired or
//! scoped bindings rather than interpreting those bindings from output names.

pub mod builder;
mod cache;
pub mod canonical_order;
pub mod case_orientation;
pub mod domains;
pub mod expr;
pub mod normalize;
pub mod primitive_enum;
mod profile;
pub mod sort_context;
pub mod symvar;

pub use canonical_order::{
    canonical_let_plan, canonical_order_compare, canonical_order_key,
    canonical_order_ordered_comparison, let_arithmetic_needs_swap, let_eager_children,
    named_parameter_key, wrap_choices, CanonicalKey, ChoiceOperands, ValuePlan,
};
pub use domains::primitive::registry::{
    func_name, func_semantics, register_func, FuncKey, FuncSemantics,
};
pub use expr::{BinOpKind, Expr, Lit, RelationAggregateKind, UnaryOpKind};
pub use normalize::{
    canonicalize_bound_integer_ops, normalize, normalize_with, try_normalize_with, BudgetKind,
    NormalizationLimit,
};
pub use primitive_enum::Primitive;
pub use sort_context::{
    CaseArmKeys, DefaultSortContext, HeapBindingIdentity, ObservationIdentity, SortContext,
};
pub use symvar::{ScalarSymbolType, SymVarOrigin};

/// Exact mathematical integer, distinct from source-language machine words.
pub use num_bigint::BigInt as Integer;
