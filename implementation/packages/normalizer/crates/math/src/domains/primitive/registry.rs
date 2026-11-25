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

    /// A closed symbolic equation supplied with this exact function signature.
    /// Restricted to pure functions returning Boolean or String; String
    /// equations preserve their result sort. Other sorts
    /// fail explicitly rather than losing complete type metadata.
    /// Every accepted rewrite must
    /// strictly decrease the normalizer's registered-call argument rank;
    /// recursive/cyclic definitions are reported as internal contract errors.
    pub symbolic_reducer:
        Option<Arc<dyn Fn(FuncKey, &[crate::Expr]) -> Option<crate::Expr> + Send + Sync>>,

}

#[derive(Clone, Debug)]
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

    /// Attach an equation to the already declared closed signature. This does
    /// not certify an otherwise unknown function for final PERS admission.
    pub fn with_symbolic_reducer(
        mut self,
        reducer: Arc<dyn Fn(FuncKey, &[crate::Expr]) -> Option<crate::Expr> + Send + Sync>,
    ) -> Self {
        self.symbolic_reducer = Some(reducer);
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
    pub param_types: Vec<crate::primitive_enum::Primitive>,
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

/// 注册一个外部函数（Primitive Functions）。
///
/// ## 参数类型强制 primitive（用户 20260428122100 明令）
///
/// `param_types` 必须是 **封闭枚举 `Primitive` 列表**。编译器强制：
/// - 不允许 Object / 泛型 T/E/K/V / 业务 Pojo
/// - 只有 primitive 演算函数能注册 Primitive Functions
///
/// 这是"Primitive Functions 红线"从人工自律升级为编译器强制的核心机制。违规 Primitive Functions 将编译失败，
/// 按清单整改（走 hack / $normalizer 后门）。
///
/// ## 行为
///
/// - 同 `(name, param_types)` 重复注册返回同一 key（幂等）
/// - 同 `(name, param_types)` 不同 return_type 或 semantics 重复注册 **panic**（防止语义冲突）
///
/// 返回的 FuncKey 是唯一的身份凭证，之后构造 Call 必须用它。
pub fn register_func(
    name: &str,
    param_types: Vec<crate::primitive_enum::Primitive>,
    return_type: Option<crate::primitive_enum::Primitive>,
    semantics: FuncSemantics,
) -> FuncKey {
    let key_hint = (name.to_string(), param_types.clone());
    let reg = registry();
    {
        let r = reg.read().unwrap();
        if let Some(&existing) = r.name_arity_index.get(&key_hint) {
            let entry = &r.entries[existing.0 as usize];
            if entry.return_type != return_type {
                panic!(
                    "[MATH-REGISTRY] conflicting registration for ({}, param_types={:?}): existing return_type={:?} != new return_type={:?}",
                    name, param_types, entry.return_type, return_type
                );
            }
            if !semantics_equivalent(&entry.semantics, &semantics) {
                panic!(
                    "[MATH-REGISTRY] conflicting registration for ({}, param_types={:?}): existing semantics != new semantics",
                    name, param_types
                );
            }
            return existing;
        }
    }
    let mut w = reg.write().unwrap();
    if let Some(&existing) = w.name_arity_index.get(&key_hint) {
        let entry = &w.entries[existing.0 as usize];
        let compatible =
            entry.return_type == return_type && semantics_equivalent(&entry.semantics, &semantics);
        drop(w);
        assert!(
            compatible,
            "[MATH-REGISTRY] conflicting concurrent registration"
        );
        return existing;
    }
    let idx = w.entries.len() as u32;
    let key = FuncKey(idx);
    w.entries.push(FuncEntry {
        name: name.to_string(),
        param_types: param_types.clone(),
        return_type,
        semantics,
    });
    w.name_arity_index.insert(key_hint, key);
    key
}

fn semantics_equivalent(a: &FuncSemantics, b: &FuncSemantics) -> bool {
    a.pure == b.pure
    // Concrete evaluators and equations are installed by their owning domains.
}

/// 查询 FuncKey 对应的函数名（用于 Display）。
pub fn func_name(key: FuncKey) -> String {
    let r = registry().read().unwrap();
    r.entries[key.0 as usize].name.clone()
}

/// 查询 FuncKey 对应的 arity（从 param_types 列表长度推断）。
pub fn func_arity(key: FuncKey) -> usize {
    let r = registry().read().unwrap();
    r.entries[key.0 as usize].param_types.len()
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
