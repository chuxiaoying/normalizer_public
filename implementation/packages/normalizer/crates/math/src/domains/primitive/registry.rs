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
    /// 是否为纯函数（相同输入 → 相同输出，无副作用）。
    /// 纯函数允许常量折叠：f(Lit, Lit) → 求值后 Lit（需要 evaluator 支持）
    pub pure: bool,

    /// 对 Null 输入的已知行为（如 isBlank(null) = true）。
    /// 语义：如果所有 args 中位置 `null_trigger_positions` 之一为 Null，则结果为 `null_result`
    pub null_behavior: Option<NullBehavior>,

    /// 可选的 pure_function 求值器（P2 方案 B）。
    ///
    /// 签名：`&[Lit] -> Option<Lit>`
    /// - 输入：全部 args 的 Lit 值（由 math::constant_fold 在 pure && all-Lit 条件下收集）
    /// - 输出 Some(lit)：折叠成功
    /// - 输出 None：evaluator 不支持此输入组合（如类型不匹配），保留原 Call
    ///
    /// 只有 pure=true 时才会被 math::constant_fold 调用。
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
    /// 纯函数：相同输入→相同输出，无副作用
    pub fn pure_function() -> Self {
        Self {
            pure: true,
            ..Default::default()
        }
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

/// Func 注册条目。
///
/// ## 参数类型强制 primitive（用户 20260428122100 明令）
///
/// 用户原话："NCL构造需要传入参数类型列表，且类型不能是泛类型，必须是primitive枚举。
/// 这样就可以逼出所有违规操作的编译错误。全部肃清。"
///
/// `param_types` 是 **封闭枚举 `Primitive` 列表**，不允许 Object / 泛型 / 业务类。
/// 这样编译器强制：只有 primitive 演算函数能注册 Primitive Functions。
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
    assert!(entry.return_type == return_type
        && entry.semantics.pure == semantics.pure
        && entry.semantics.null_behavior == semantics.null_behavior
        && same_evaluator,
        "[NORM-INVARIANT] conflicting contracts for canonical operation {}", entry.name);
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
    let key = FuncKey(u32::try_from(writer.entries.len())
        .expect("[NORM-ANALYSIS-INCOMPLETE] primitive contract registry exhausted"));
    writer.entries.push(FuncEntry { name: name.to_owned(), param_types, return_type, semantics });
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

pub(crate) fn func_signature(key: FuncKey) -> (String, Vec<crate::Primitive>, Option<crate::Primitive>) {
    let registry = registry().read().unwrap();
    let entry = &registry.entries[key.0 as usize];
    (entry.name.clone(), entry.param_types.clone(), entry.return_type)
}
