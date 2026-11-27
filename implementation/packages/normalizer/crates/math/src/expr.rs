//! Expr AST — 表达式类型。
//!
//! ## 收口约束
//!
//! - `Expr` 本身是不透明的 newtype，内部 `ExprKind` 不对外暴露
//! - 所有构造必须通过 `crate::builder` 的工厂函数
//! - 字段私有，外部无法通过字段访问绕过工厂
//!
//! 这保证了：
//! - 裸 `Expr::Call { ... }` 在模块外不可能
//! - 未通过注册表的 Func 不能出现在 Expr 中
//! - SymVar 的 origin 不能凭空构造

use crate::domains::primitive::registry::FuncKey;
use crate::symvar::SymVarOrigin;
use std::collections::hash_map::DefaultHasher;
use std::collections::BTreeSet;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

/// 表达式类型。不透明，字段私有。
///
/// 构造请使用 `crate::builder` 的工厂函数。
///
/// `Expr` immutable：`kind` 与 `profile` 均在构造时写入 Rc，之后只读共享。
/// `profile` 是表达式内部透视 cache，复杂度/深度等构造期预计算，查询必须 O(1)。
#[derive(Clone, Debug)]
pub struct Expr(pub(crate) Rc<ExprKind>, pub(crate) Rc<ExprProfile>);

/// 表达式内部透视 cache。构造 Expr 时由子表达式 profile 合并预计算。
///
/// 后续可扩展 vars/has_heap_ref/has_case_when 等索引；复杂度检查和化简快捷分析
/// 禁止回到每次递归扫描整棵树。
#[derive(Clone)]
pub struct ExprProfile {
    size: usize,
    depth: usize,
    vars: BTreeSet<String>,
    has_heap_ref: bool,
    has_case_when: bool,
    has_relation_aggregate: bool,
    structural_hash: u64,
}

// The derived aggregate hint is not part of the original Debug signature;
// propositional proof atom keys use that signature.
impl fmt::Debug for ExprProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExprProfile")
            .field("size", &self.size)
            .field("depth", &self.depth)
            .field("vars", &self.vars)
            .field("has_heap_ref", &self.has_heap_ref)
            .field("has_case_when", &self.has_case_when)
            .field("structural_hash", &self.structural_hash)
            .finish()
    }
}

/// 字面量类型。
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Lit {
    Int(i64),
    /// Exact mathematical integer outside the compact i64 representation.
    BigInt(num_bigint::BigInt),
    Str(String),
    Bool(bool),
    Null,
}

impl Lit {
    /// Mathematical integer value; storage width does not define its domain.
    pub fn as_integer(&self) -> Option<num_bigint::BigInt> {
        match self {
            Self::Int(n) => Some((*n).into()),
            Self::BigInt(n) => Some(n.clone()),
            _ => None,
        }
    }
}

/// 二元运算符。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BinOpKind {
    Add,
    Sub,
    Mul,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

/// The closed comparison subset of binary operators.
#[derive(Clone, Copy)]
pub(crate) enum Comparison {
    Eq, Ne, Lt, Le, Gt, Ge,
}

impl Comparison {
    pub(crate) fn swapped(self) -> Self {
        use Comparison::*;
        match self {
            Lt => Gt, Le => Ge, Gt => Lt, Ge => Le, Eq | Ne => self,
        }
    }

    fn complemented(self) -> Self {
        use Comparison::*;
        match self {
            Eq => Ne, Ne => Eq, Lt => Ge, Ge => Lt, Le => Gt, Gt => Le,
        }
    }

    fn test<T: Ord>(self) -> fn(&T, &T) -> bool {
        use Comparison::*;
        match self {
            Eq => T::eq, Ne => T::ne, Lt => T::lt, Le => T::le, Gt => T::gt, Ge => T::ge,
        }
    }
}

impl From<Comparison> for BinOpKind {
    fn from(comparison: Comparison) -> Self {
        use Comparison::*;
        match comparison {
            Eq => Self::Eq, Ne => Self::Ne, Lt => Self::Lt, Le => Self::Le,
            Gt => Self::Gt, Ge => Self::Ge,
        }
    }
}

impl BinOpKind {
    pub(crate) fn comparison(self) -> Option<Comparison> {
        use BinOpKind::*;
        Some(match self {
            Eq => Comparison::Eq, Ne => Comparison::Ne, Lt => Comparison::Lt,
            Le => Comparison::Le, Gt => Comparison::Gt, Ge => Comparison::Ge,
            Add | Sub | Mul | And | Or => return None,
        })
    }

    pub(crate) fn complemented_comparison(self) -> Option<Self> {
        Some(self.comparison()?.complemented().into())
    }

    pub(crate) fn comparison_test<T: Ord>(self) -> Option<fn(&T, &T) -> bool> {
        Some(self.comparison()?.test())
    }

    /// Equivalent comparison after exchanging the already evaluated operands.
    /// This does not negate the predicate or apply to arithmetic or logic.
    pub fn swapped_comparison(self) -> Option<Self> {
        Some(self.comparison()?.swapped().into())
    }
}

/// 一元运算符。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UnaryOpKind {
    Neg,
    Not,
}

/// 关系聚合运算。表达结构化 RCL relation 上的摘要查询。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RelationAggregateKind {
    Count,
    Sum,
    FirstValue,
    NthValue,
    Exists,
}

/// 表达式内部结构。pub(crate) 限制在 math crate 内。
///
#[derive(Clone, Debug)]
pub(crate) enum ExprKind {
    /// 字面量常量
    Lit(Lit),

    /// 符号变量，origin 标识来源（P/E/T）
    SymVar { origin: SymVarOrigin },

    /// 堆引用：指向实时内核对象身份分配器中的对象 ID。
    ///
    /// 设计说明：HeapRef 是符号执行中的"具体引用值"，与 Lit/SymVar 并列。
    /// ID 由 executor 的 `Heap::new_object` 发放，math 只持 u64 不负责解引用。
    /// HeapRef 不参与代数化简——normalize 各步骤对 HeapRef 一律 passthrough。
    HeapRef(i64),

    /// Relation row value atom.
    ///
    /// This is not a Java parameter/SymVar and therefore carries no fake Pojo
    /// primitive type.  It denotes the current row value/path inside a Relation
    /// Calculus predicate or projection, e.g. `$e_0.item` rendered as SQL alias
    /// `p` under relation formatting scope.
    RowValue { alias: String, acquired_integer: bool, sql: Option<SqlColumnBinding> },

    /// 调用签名由 FuncKey 唯一决定。
    Call { key: FuncKey, args: Vec<Expr> },

    /// 二元运算
    BinOp {
        op: BinOpKind,
        left: Expr,
        right: Expr,
    },

    /// 一元运算
    UnaryOp { op: UnaryOpKind, operand: Expr },

    /// 分支表达式。每个分支是 (condition, value)，
    /// 最后一个分支的 condition 应为 Lit(Bool(true))（默认分支）。
    CaseWhen { branches: Vec<(Expr, Expr)> },

    /// 数组字面量。items 是有序的元素列表。
    ///
    /// 设计说明：ArrayLit 是符号执行中表达 Java `new T[]{a, b, c}` 或
    /// varargs 打包 `foo(a, b, c)` → `foo(new T[]{a, b, c})` 的结构化值。
    /// 不参与代数化简——normalize 对 ArrayLit 只做 items 内部递归化简，
    /// 不做跨 item 合并/等价变换。
    ArrayLit(Vec<Expr>),

    /// Relation aggregate query over a structured RCL relation.
    ///
    /// This node keeps the relation AST in Expr instead of collapsing it into
    /// legacy source/where strings.
    RelationAggregate {
        kind: RelationAggregateKind,
        relation: Box<crate::domains::relation::RelationExpr>,
        select_expr: Option<Expr>,
        default_expr: Option<Expr>,
    },

    /// Relation-valued expression node.
    ///
    /// RCL is part of the single math Expr IR.  Relation operators live here as
    /// Expr nodes; their predicates/projections remain ordinary Expr values.
    Relation(Box<crate::domains::relation::RelationExpr>),
}

/// Physical column identity, separate from its rendered SQL qualifier.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct SqlColumnBinding {
    pub(crate) table: crate::domains::relation::TableSource,
    pub(crate) row_slot: usize,
    pub(crate) column: String,
    pub(crate) scalar_type: &'static str,
}

// ===== 手写 PartialEq/Eq/Hash =====
//
// 其他变体按字段结构正常比较。

impl PartialEq for ExprKind {
    fn eq(&self, other: &Self) -> bool {
        use ExprKind::*;
        match (self, other) {
            (Lit(a), Lit(b)) => a == b,
            (SymVar { origin: a }, SymVar { origin: b }) => a == b,
            (HeapRef(a), HeapRef(b)) => a == b,
            (RowValue { alias: a, acquired_integer: left, sql: sql_left },
                RowValue { alias: b, acquired_integer: right, sql: sql_right }) =>
                a == b && left == right && sql_left == sql_right,
            (
                Call {
                    key: k1,
                    args: a1,
                },
                Call {
                    key: k2,
                    args: a2,
                },
            ) => k1 == k2 && a1 == a2,
            (
                BinOp {
                    op: o1,
                    left: l1,
                    right: r1,
                },
                BinOp {
                    op: o2,
                    left: l2,
                    right: r2,
                },
            ) => o1 == o2 && l1 == l2 && r1 == r2,
            (
                UnaryOp {
                    op: o1,
                    operand: op1,
                },
                UnaryOp {
                    op: o2,
                    operand: op2,
                },
            ) => o1 == o2 && op1 == op2,
            (CaseWhen { branches: a }, CaseWhen { branches: b }) => a == b,
            (ArrayLit(a), ArrayLit(b)) => a == b,
            (
                RelationAggregate {
                    kind: k1,
                    relation: r1,
                    select_expr: se1,
                    default_expr: d1,
                },
                RelationAggregate {
                    kind: k2,
                    relation: r2,
                    select_expr: se2,
                    default_expr: d2,
                },
            ) => k1 == k2 && r1 == r2 && se1 == se2 && d1 == d2,
            (Relation(a), Relation(b)) => a == b,
            _ => false, // 不同变体不等
        }
    }
}

impl Eq for ExprKind {}

impl PartialEq for Expr {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for Expr {}

impl Hash for Expr {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Expr is immutable: structural_hash is computed once at construction.
        // Eq remains full structural equality; this only avoids recursively
        // hashing large expression trees in high-frequency memo/cache lookups.
        self.1.structural_hash.hash(state);
    }
}

impl Expr {
    /// math crate 内部唯一原始构造入口：创建 immutable Expr，并在构造期预计算 profile。
    pub(crate) fn new(kind: ExprKind) -> Self {
        let profile = ExprProfile::from_kind(&kind);
        Expr(Rc::new(kind), Rc::new(profile))
    }

    /// 内部访问 kind，仅在 math crate 内部使用
    pub(crate) fn kind(&self) -> &ExprKind {
        self.0.as_ref()
    }

    // ===== 公开查询器 =====
    //
    // 这些方法让 math 的外部使用者（executor/plugin 等）不需要裸匹配 ExprKind
    // 就能判断 Expr 的形态。符合 v2 封装原则。

    /// 如果是 HeapRef，返回内部 id；否则 None。
    pub fn as_heap_ref(&self) -> Option<i64> {
        match self.0.as_ref() {
            ExprKind::HeapRef(id) => Some(*id),
            _ => None,
        }
    }

    /// 如果是 Relation row value atom，返回 row alias/path；否则 None。
    pub fn as_row_value(&self) -> Option<&str> {
        match self.0.as_ref() {
            ExprKind::RowValue { alias, .. } => Some(alias.as_str()),
            _ => None,
        }
    }

    /// 如果是字面量，返回字面量引用；否则 None。
    pub fn as_lit(&self) -> Option<&Lit> {
        match self.0.as_ref() {
            ExprKind::Lit(l) => Some(l),
            _ => None,
        }
    }

    /// 如果是符号变量，返回其 origin；否则 None。
    pub fn as_sym_var(&self) -> Option<&SymVarOrigin> {
        match self.0.as_ref() {
            ExprKind::SymVar { origin } => Some(origin),
            _ => None,
        }
    }

    /// 是否是 Lit(Null)。
    pub fn is_null(&self) -> bool {
        matches!(self.0.as_ref(), ExprKind::Lit(Lit::Null))
    }

    /// 如果是 Lit::Bool，返回 Some(bool)；否则 None。
    ///
    /// 用于 executor 的 if short-circuit：cond 是常量 bool 时不需要宇宙分裂，
    /// 直接走对应分支。与 is_null 风格一致。
    pub fn as_bool(&self) -> Option<bool> {
        match self.0.as_ref() {
            ExprKind::Lit(Lit::Bool(b)) => Some(*b),
            _ => None,
        }
    }

    /// 如果是 Call，返回 (FuncKey, args 切片)；否则 None。
    ///
    /// FuncKey 是值类型（内部为 u32 id 包装），拷贝成本可忽略。
    /// args 借用，生命周期与 Expr 一致。
    pub fn as_call(&self) -> Option<(FuncKey, &[Expr])> {
        match self.0.as_ref() {
            ExprKind::Call { key, args, .. } => Some((*key, args.as_slice())),
            _ => None,
        }
    }

    /// 如果是 ArrayLit，返回 items 切片；否则 None。
    ///
    /// items 借用，生命周期与 Expr 一致。外部使用者通过此 API 获取
    /// 数组字面量的元素列表，无需裸匹配 ExprKind。
    pub fn as_array_lit(&self) -> Option<&[Expr]> {
        match self.0.as_ref() {
            ExprKind::ArrayLit(items) => Some(items.as_slice()),
            _ => None,
        }
    }

    /// 如果是 CaseWhen，返回 branches 切片；否则 None。
    ///
    /// branches 借用，生命周期与 Expr 一致。外部使用者通过此 API 获取
    /// 分支表达式的 (condition, value) 列表，无需裸匹配 ExprKind。
    ///
    /// PR 2 新增：formatting::sql 层 CaseWhen SQL format 能力补齐所需。
    pub fn as_case_when(&self) -> Option<&[(Expr, Expr)]> {
        match self.0.as_ref() {
            ExprKind::CaseWhen { branches } => Some(branches.as_slice()),
            _ => None,
        }
    }

    /// Read a CASE with an explicit, unconditional final result.
    pub fn as_total_case_when(&self) -> Option<(&[(Expr, Expr)], &Expr)> {
        let ((guard, fallback), arms) = self.as_case_when()?.split_last()?;
        (guard.as_bool() == Some(true)).then_some((arms, fallback))
    }

    /// 如果是 UnaryOp，返回 (op, operand)；否则 None。
    ///
    /// 外部使用者（如 formatting::sql）通过此 API 获取一元运算表达式的内部，
    /// 无需裸匹配 ExprKind。
    ///
    /// PR 2 新增：formatting::sql 层 UnaryOp SQL format 能力补齐所需。
    pub fn as_unary_op(&self) -> Option<(UnaryOpKind, &Expr)> {
        match self.0.as_ref() {
            ExprKind::UnaryOp { op, operand } => Some((*op, operand)),
            _ => None,
        }
    }

    /// Return the Boolean NOT operand, never a numeric negation operand.
    pub(crate) fn as_not(&self) -> Option<&Expr> {
        match self.kind() {
            ExprKind::UnaryOp { op: UnaryOpKind::Not, operand } => Some(operand),
            _ => None,
        }
    }

    /// Comparison tokens are only mathematical complements under the
    /// operand contract. Ordered comparisons over opaque or unordered values
    /// retain a Boolean NOT rather than inventing a closed number order.
    pub(crate) fn comparison_complement_operator(&self) -> Option<BinOpKind> {
        let (op, left, right) = self.as_binop()?;
        use BinOpKind::*;
        match op {
            Eq | Ne => op.complemented_comparison(),
            Lt | Le | Gt | Ge if left.has_exact_numeric_order() && right.has_exact_numeric_order() =>
                op.complemented_comparison(),
            _ => None,
        }
    }

    // Value contract only. This cannot authorize eliding or moving a read;
    // replacing a comparison token still evaluates these same two operands.
    fn has_exact_numeric_order(&self) -> bool {
        use crate::domains::decimal::{self, View};
        match self.kind() {
            ExprKind::Lit(Lit::Int(_) | Lit::BigInt(_)) => true,
            ExprKind::SymVar { origin } =>
                matches!(origin.primitive_type(), "integer" | decimal::VALUE_TAG),
            ExprKind::RowValue { acquired_integer, sql, .. } =>
                *acquired_integer || sql.as_ref().is_some_and(|column| column.scalar_type == "number"),
            ExprKind::UnaryOp { op: UnaryOpKind::Neg, operand } => operand.has_exact_numeric_order(),
            ExprKind::BinOp { op: BinOpKind::Add | BinOpKind::Sub | BinOpKind::Mul, left, right } =>
                left.has_exact_numeric_order() && right.has_exact_numeric_order(),
            ExprKind::RelationAggregate { kind: RelationAggregateKind::Count, .. } => true,
            ExprKind::Call { .. } => match decimal::view(self) {
                Some(View::Literal(..)) => true,
                Some(View::Multiply(left, right)) => left.has_exact_numeric_order() && right.has_exact_numeric_order(),
                Some(View::Round(value, ..)) => value.has_exact_numeric_order(),
                None => crate::domains::composite::vector_length_input(self).is_some(),
            },
            _ => false,
        }
    }

    /// A nullable physical column can produce SQL UNKNOWN. Stop at
    /// aggregate/relationship boundaries: an acquired COUNT is not a row value.
    pub(crate) fn may_observe_sql_unknown(&self) -> bool {
        let mut nullable = false;
        self.walk_pruned(&mut |node| match node.kind() {
            ExprKind::RelationAggregate { .. } | ExprKind::Relation(_) => false,
            ExprKind::RowValue { sql: Some(column), .. } => {
                nullable |= column.scalar_type.ends_with('?');
                false
            }
            _ => !nullable,
        });
        nullable
    }

    /// Complementary tokens are not an exhaustive Boolean partition when
    /// comparison of the same operands can produce SQL UNKNOWN.
    pub(crate) fn is_comparison_complement(&self, other: &Expr) -> bool {
        let Some((_, left, right)) = self.as_binop() else { return false; };
        let Some((b, other_left, other_right)) = other.as_binop() else { return false; };
        left == other_left && right == other_right && self.comparison_complement_operator() == Some(b)
            && !self.may_observe_sql_unknown()
    }

    /// 查询器：若本 Expr 是 BinOp，返回 (op, left, right)
    ///
    /// PR 2 新增：formatting::sql 层 BinOp SQL format 能力补齐所需。
    /// executor 产出的 BinOp 是独立变体（不走 Call/FuncKey 路径），
    /// formatting::sql 需要直接匹配 ExprKind::BinOp 输出中缀形式。
    pub fn as_binop(&self) -> Option<(BinOpKind, &Expr, &Expr)> {
        match self.0.as_ref() {
            ExprKind::BinOp { op, left, right } => Some((*op, left, right)),
            _ => None,
        }
    }

    /// 如果是 Relation-valued Expr，返回 relation 结构；否则 None。
    pub fn as_relation(&self) -> Option<&crate::domains::relation::RelationExpr> {
        match self.0.as_ref() {
            ExprKind::Relation(relation) => Some(relation.as_ref()),
            _ => None,
        }
    }

    /// 如果是结构化 RelationAggregate，返回关系聚合字段；否则 None。
    pub fn as_relation_aggregate(
        &self,
    ) -> Option<(
        RelationAggregateKind,
        &crate::domains::relation::RelationExpr,
        Option<&Expr>,
        Option<&Expr>,
    )> {
        match self.0.as_ref() {
            ExprKind::RelationAggregate {
                kind,
                relation,
                select_expr,
                default_expr,
            } => Some((
                *kind,
                relation.as_ref(),
                select_expr.as_ref(),
                default_expr.as_ref(),
            )),
            _ => None,
        }
    }

    /// O(1) 返回构造期预计算的表达式节点数（用于路径复杂度度量）。
    pub fn size(&self) -> usize {
        self.1.size
    }

    /// O(1) 返回构造期预计算的表达式深度。
    pub fn depth(&self) -> usize {
        self.1.depth
    }

    /// O(1) 返回构造期预计算的符号变量集合。
    pub fn vars(&self) -> &BTreeSet<String> {
        &self.1.vars
    }

    /// O(1) 查询表达式是否包含 HeapRef。
    pub fn has_heap_ref(&self) -> bool {
        self.1.has_heap_ref
    }

    /// O(1) test for a lazy relation observation in this expression tree.
    /// A bare relation is not itself an observation of its rows.
    pub(crate) fn contains_relation_aggregate(&self) -> bool {
        self.1.has_relation_aggregate
    }

    /// O(1) 查询表达式是否包含 CaseWhen。
    pub fn has_case_when(&self) -> bool {
        self.1.has_case_when
    }

    /// O(1) structural hash precomputed at construction time.
    ///
    /// This is a cache for HashMap/HashSet lookups over immutable Expr trees.
    /// It is not an identity shortcut: `Eq` remains full structural equality.
    pub fn structural_hash(&self) -> u64 {
        self.1.structural_hash
    }

    /// Visit conjunction leaves in original left-to-right order.
    pub fn for_each_conjunct<'a>(&'a self, mut visit: impl FnMut(&'a Expr)) {
        self.visit_operator_leaves(BinOpKind::And, &mut visit);
    }

    pub(crate) fn visit_operator_leaves<'a>(
        &'a self,
        target: BinOpKind,
        visit: &mut impl FnMut(&'a Expr),
    ) {
        if let Some((op, left, right)) = self.as_binop() {
            if op == target {
                left.visit_operator_leaves(target, visit);
                right.visit_operator_leaves(target, visit);
                return;
            }
        }
        visit(self);
    }

    /// Walk all sub-expressions (including self) depth-first, calling `f` on each.
    pub fn walk<F: FnMut(&Expr)>(&self, f: &mut F) {
        self.walk_pruned(&mut |node| { f(node); true });
    }

    /// Walk depth-first, descending only when the visitor accepts this node.
    pub fn walk_pruned<F: FnMut(&Expr) -> bool>(&self, f: &mut F) {
        if !f(self) { return; }
        match self.kind() {
            ExprKind::Lit(_)
            | ExprKind::SymVar { .. }
            | ExprKind::HeapRef(_)
            | ExprKind::RowValue { .. } => {}
            ExprKind::BinOp { left, right, .. } => {
                left.walk_pruned(f);
                right.walk_pruned(f);
            }
            ExprKind::UnaryOp { operand, .. } => {
                operand.walk_pruned(f);
            }
            ExprKind::CaseWhen { branches } => {
                for (cond, val) in branches {
                    cond.walk_pruned(f);
                    val.walk_pruned(f);
                }
            }
            ExprKind::Call { args, .. } => {
                for arg in args {
                    arg.walk_pruned(f);
                }
            }
            ExprKind::ArrayLit(items) => {
                for item in items {
                    item.walk_pruned(f);
                }
            }
            ExprKind::RelationAggregate {
                relation,
                select_expr,
                default_expr,
                ..
            } => {
                walk_relation_expr(relation, f);
                if let Some(se) = select_expr {
                    se.walk_pruned(f);
                }
                if let Some(de) = default_expr {
                    de.walk_pruned(f);
                }
            }
            ExprKind::Relation(rel) => {
                walk_relation_expr(rel, f);
            }
        }
    }
}

/// Walk all Expr nodes inside a RelationExpr.
fn walk_relation_expr<F: FnMut(&Expr) -> bool>(rel: &crate::domains::relation::RelationExpr, f: &mut F) {
    use crate::domains::relation::RelationExpr;
    match rel {
        RelationExpr::Empty | RelationExpr::Base { .. } | RelationExpr::ProjectedUnion(_)
        | RelationExpr::Grouped(_) | RelationExpr::RecordProjection { .. } => {}
        RelationExpr::Vector { value, .. } => value.walk_pruned(f),
        RelationExpr::Select { predicate, input } => {
            predicate.walk_pruned(f);
            walk_relation_expr(input, f);
        }
        RelationExpr::Project { value, input, .. } => {
            value.walk_pruned(f);
            walk_relation_expr(input, f);
        }
        RelationExpr::Join { on, filters, .. } => {
            on.walk_pruned(f);
            filters.iter().for_each(|filter| filter.walk_pruned(f));
        }
        RelationExpr::Distinct(inner) => {
            walk_relation_expr(inner, f);
        }
    }
}

impl ExprProfile {
    fn leaf() -> Self {
        Self {
            size: 1,
            depth: 1,
            vars: BTreeSet::new(),
            has_heap_ref: false,
            has_case_when: false,
            has_relation_aggregate: false,
            structural_hash: 0,
        }
    }

    fn sym_var(origin: &SymVarOrigin) -> Self {
        let mut p = Self::leaf();
        p.vars.insert(sym_var_profile_key(origin));
        p
    }

    fn heap_ref() -> Self {
        Self {
            has_heap_ref: true,
            ..Self::leaf()
        }
    }

    fn row_value(alias: &str) -> Self {
        let mut p = Self::leaf();
        p.vars.insert(alias.to_string());
        p
    }

    fn include_child_shape(&mut self, child: &Self) {
        self.size += child.size;
        self.depth = self.depth.max(1 + child.depth);
        self.has_heap_ref |= child.has_heap_ref;
        self.has_case_when |= child.has_case_when;
        self.has_relation_aggregate |= child.has_relation_aggregate;
    }

    fn include_child(&mut self, child: &Self) {
        self.include_child_shape(child);
        for var in &child.vars {
            if !self.vars.contains(var) {
                self.vars.insert(var.clone());
            }
        }
    }

    fn from_children<'a>(children: impl IntoIterator<Item = &'a Self>) -> Self {
        let mut parent = Self::leaf();
        for child in children {
            parent.include_child(child);
        }
        parent
    }

    fn wrapped(mut self) -> Self {
        self.size += 1;
        self.depth += 1;
        self
    }

    fn relation(relation: &crate::domains::relation::RelationExpr) -> Self {
        use crate::domains::relation::RelationExpr as R;
        match relation {
            R::Empty => Self::leaf(),
            R::Base { name, alias, .. } => {
                let mut p = Self::leaf();
                p.vars.insert(name.clone());
                p.vars.insert(alias.clone());
                p
            }
            R::Vector { value, alias } => {
                let mut p = Self::from_children([value.1.as_ref()]);
                p.vars.insert(alias.clone());
                p
            }
            R::Select { predicate: value, input } | R::Project { value, input, .. } => {
                let mut parent = Self::relation(input).wrapped();
                parent.include_child(value.1.as_ref());
                parent
            }
            R::Distinct(input) => Self::relation(input).wrapped(),
            R::Join { left, right, on, filters } => {
                let mut p = Self::from_children(std::iter::once(on).chain(filters).map(|term| term.1.as_ref()));
                for (table, alias) in [left, right] {
                    p.vars.insert(table.name.clone());
                    p.vars.insert(alias.clone());
                }
                p
            }
            R::RecordProjection { source, field } => {
                let mut p = Self::leaf();
                p.vars.insert(source.clone());
                p.vars.insert(format!("r.{field}"));
                p
            }
            R::Grouped(group) => {
                let mut p = Self::leaf();
                p.vars.insert(group.table().name.clone());
                p.vars.extend(group.columns().map(|column| format!("{}.{column}", group.table().name)));
                p
            }
            R::ProjectedUnion(columns) => {
                let mut parent = Self::leaf();
                for (table, column) in columns.iter() {
                    parent.vars.insert(table.name.clone());
                    parent.vars.insert(format!("{}.{column}", table.name));
                }
                parent
            }
        }
    }

    fn from_kind(kind: &ExprKind) -> Self {
        let mut profile = match kind {
            ExprKind::Lit(_) => Self::leaf(),
            ExprKind::SymVar { origin } => Self::sym_var(origin),
            ExprKind::HeapRef(_) => Self::heap_ref(),
            ExprKind::RowValue { alias, .. } => Self::row_value(alias),
            ExprKind::Call { args, .. } | ExprKind::ArrayLit(args) =>
                Self::from_children(args.iter().map(|arg| arg.1.as_ref())),
            ExprKind::BinOp { left, right, .. } =>
                Self::from_children([left.1.as_ref(), right.1.as_ref()]),
            ExprKind::UnaryOp { operand, .. } => Self::from_children([operand.1.as_ref()]),
            ExprKind::CaseWhen { branches } => {
                let mut p = Self::from_children(
                    branches.iter().flat_map(|(c, v)| [c.1.as_ref(), v.1.as_ref()]),
                );
                p.has_case_when = true;
                p
            }
            ExprKind::RelationAggregate {
                relation,
                select_expr,
                default_expr,
                ..
            } => {
                let mut profile = Self::relation(relation).wrapped();
                for expr in select_expr.iter().chain(default_expr.iter()) {
                    profile.include_child(expr.1.as_ref());
                }
                profile.has_relation_aggregate = true;
                profile
            }
            ExprKind::Relation(relation) => {
                let mut profile = Self::relation(relation);
                // Keep size/depth/vars, but do not expose observations inside
                // a bare relation or propagate them to its Expr parents.
                profile.has_relation_aggregate = false;
                profile
            }
        };
        profile.structural_hash = structural_hash_from_kind(kind);
        profile
    }
}

fn structural_hash_from_kind(kind: &ExprKind) -> u64 {
    let mut h = DefaultHasher::new();
    use ExprKind::*;
    match kind {
        Lit(v) => {
            0u8.hash(&mut h);
            v.hash(&mut h);
        }
        SymVar { origin } => {
            1u8.hash(&mut h);
            origin.hash(&mut h);
        }
        HeapRef(id) => {
            2u8.hash(&mut h);
            id.hash(&mut h);
        }
        RowValue { alias, acquired_integer, sql } => {
            11u8.hash(&mut h);
            alias.hash(&mut h);
            acquired_integer.hash(&mut h);
            sql.hash(&mut h);
        }
        Call {
            key,
            args,
        } => {
            3u8.hash(&mut h);
            key.hash(&mut h);
            for arg in args {
                arg.structural_hash().hash(&mut h);
            }
        }
        BinOp { op, left, right } => {
            4u8.hash(&mut h);
            op.hash(&mut h);
            left.structural_hash().hash(&mut h);
            right.structural_hash().hash(&mut h);
        }
        UnaryOp { op, operand } => {
            5u8.hash(&mut h);
            op.hash(&mut h);
            operand.structural_hash().hash(&mut h);
        }
        CaseWhen { branches } => {
            6u8.hash(&mut h);
            for (cond, value) in branches {
                cond.structural_hash().hash(&mut h);
                value.structural_hash().hash(&mut h);
            }
        }
        ArrayLit(items) => {
            7u8.hash(&mut h);
            for item in items {
                item.structural_hash().hash(&mut h);
            }
        }
        RelationAggregate {
            kind,
            relation,
            select_expr,
            default_expr,
        } => {
            9u8.hash(&mut h);
            kind.hash(&mut h);
            relation.hash(&mut h);
            select_expr
                .as_ref()
                .map(|e| e.structural_hash())
                .hash(&mut h);
            default_expr
                .as_ref()
                .map(|e| e.structural_hash())
                .hash(&mut h);
        }
        Relation(relation) => {
            10u8.hash(&mut h);
            relation.hash(&mut h);
        }
    }
    h.finish()
}

fn sym_var_profile_key(origin: &SymVarOrigin) -> String {
    match origin {
        SymVarOrigin::Parameter { name, .. } => name.clone(),
        SymVarOrigin::External {
            binding_id, path, ..
        } => {
            if path.is_empty() {
                format!("$e_{}", binding_id)
            } else {
                format!("$e_{}.{}", binding_id, path.join("."))
            }
        }
        SymVarOrigin::Temporal { binding_id, .. } => format!("$t_{}", binding_id),
    }
}
