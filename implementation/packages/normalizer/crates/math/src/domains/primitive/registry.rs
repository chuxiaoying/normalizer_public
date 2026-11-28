//! Registered primitive operations identify their closed signatures and supported laws.
//! A Call carries the key returned by registration, not a host method name.

use crate::expr::Lit;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

/// Func 身份凭证。字段私有，无 public 构造函数。
///
/// 只能通过 `register_func()` 获得。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FuncKey(u32);

#[derive(Clone, Default)]
pub struct FuncSemantics {
    /// If an argument at a declared trigger position is null, the
    /// registered value operation can return this known result.
    pub null_behavior: Option<NullBehavior>,

    /// Optional exact evaluator for literal operands. Returning None keeps
    /// the call for the other normalization rules.
    pub evaluator: Option<Arc<dyn Fn(&[Lit]) -> Option<Lit> + Send + Sync>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NullBehavior {
    /// 哪些参数位置触发（通常是 [0]，即第一个参数）
    pub trigger_positions: Vec<usize>,
    /// 触发时的结果（常量）
    pub result: crate::expr::Lit,
}

impl FuncSemantics {
    /// Registered math calls are value operations. Environmental reads and
    /// actions are acquired by execution and never registered as evaluators.
    pub fn pure_function() -> Self {
        Self::default()
    }

    /// 组合已有语义 + null_behavior（链式构造，保留原有语义）
    pub fn with_null_behavior(
        mut self,
        trigger_positions: Vec<usize>,
        result: crate::expr::Lit,
    ) -> Self {
        self.null_behavior = Some(NullBehavior {
            trigger_positions,
            result,
        });
        self
    }

    /// 组合已有语义 + evaluator（链式构造）
    pub fn with_evaluator(
        mut self,
        evaluator: std::sync::Arc<
            dyn Fn(&[crate::expr::Lit]) -> Option<crate::expr::Lit> + Send + Sync,
        >,
    ) -> Self {
        self.evaluator = Some(evaluator);
        self
    }
}

/// A registered value operation has a closed primitive signature. Java
/// classes and generic host types do not serve as operand domains here.
#[derive(Clone)]
pub(crate) struct FuncEntry {
    pub name: String,
    param_types: Vec<crate::primitive_enum::Primitive>,
    pub return_type: Option<crate::primitive_enum::Primitive>,
    pub semantics: FuncSemantics,
}

/// 全局 Func 注册表。
struct FuncRegistry {
    entries: Vec<FuncEntry>,
    /// (name, param_types) → FuncKey，用于幂等
    /// 参数类型列表参与身份（同名不同类型视为不同 Func）
    name_arity_index: HashMap<(String, Vec<crate::primitive_enum::Primitive>), FuncKey>,
}

impl FuncRegistry {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
            name_arity_index: HashMap::new(),
        }
    }
}

fn registry() -> &'static RwLock<FuncRegistry> {
    static REG: OnceLock<RwLock<FuncRegistry>> = OnceLock::new();
    REG.get_or_init(|| RwLock::new(FuncRegistry::new()))
}

fn verify_repeated_contract(
    entry: &FuncEntry,
    return_type: Option<crate::Primitive>,
    semantics: &FuncSemantics,
) {
    let same_evaluator = match (&entry.semantics.evaluator, &semantics.evaluator) {
        (None, None) => true,
        (Some(registered), Some(supplied)) => Arc::ptr_eq(registered, supplied),
        _ => false,
    };
    assert!(
        entry.return_type == return_type
            && entry.semantics.null_behavior == semantics.null_behavior
            && same_evaluator,
        "[NORM-INVARIANT] conflicting contracts for canonical operation {}",
        entry.name
    );
}

/// 注册有明确值合同的原生运算。
///
/// A repeated name and typed signature must reuse exactly the same contract.
/// Evaluator-bearing registrations should be shared by their declaring
/// operation, not created afresh by unrelated callers.
pub fn register_func(
    name: &str,
    param_types: Vec<crate::primitive_enum::Primitive>,
    return_type: Option<crate::primitive_enum::Primitive>,
    semantics: FuncSemantics,
) -> FuncKey {
    let key_hint = (name.to_string(), param_types.clone());
    let reg = registry();
    // Recheck under the writer so concurrent first registrations share a key.
    {
        let current = reg.read().unwrap();
        if let Some(&key) = current.name_arity_index.get(&key_hint) {
            verify_repeated_contract(&current.entries[key.0 as usize], return_type, &semantics);
            return key;
        }
    }
    let mut writer = reg.write().unwrap();
    if let Some(&key) = writer.name_arity_index.get(&key_hint) {
        verify_repeated_contract(&writer.entries[key.0 as usize], return_type, &semantics);
        return key;
    }
    let key = FuncKey(
        u32::try_from(writer.entries.len())
            .expect("[NORM-ANALYSIS-INCOMPLETE] primitive contract registry exhausted"),
    );
    writer.entries.push(FuncEntry {
        name: name.to_owned(),
        param_types,
        return_type,
        semantics,
    });
    writer.name_arity_index.insert(key_hint, key);
    key
}

/// 查询 FuncKey 对应的函数名（用于 Display）。
pub fn func_name(key: FuncKey) -> String {
    let r = registry().read().unwrap();
    r.entries[key.0 as usize].name.clone()
}

/// 查询 FuncKey 对应的返回类型（None = Pending / Void）。
pub fn func_return_type(key: FuncKey) -> Option<crate::primitive_enum::Primitive> {
    let r = registry().read().unwrap();
    r.entries[key.0 as usize].return_type
}

/// 查询 FuncKey 对应的 semantics。
///
/// 对外暴露：math::constant_fold 需要 pub 访问；外部测试也可读取（读不可改）。
pub fn func_semantics(key: FuncKey) -> FuncSemantics {
    let r = registry().read().unwrap();
    r.entries[key.0 as usize].semantics.clone()
}

pub(crate) fn func_signature(
    key: FuncKey,
) -> (String, Vec<crate::Primitive>, Option<crate::Primitive>) {
    let registry = registry().read().unwrap();
    let entry = &registry.entries[key.0 as usize];
    (
        entry.name.clone(),
        entry.param_types.clone(),
        entry.return_type,
    )
}
