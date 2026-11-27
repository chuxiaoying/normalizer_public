//! SortContext trait — IoC 控制反转：math 要求排序参与者提供必要信息
//!
//! ## 设计
//!
//! math 层不依赖 pipeline/heap，但排序需要 HeapRef 的 display_name。
//! 通过 trait 让 pipeline 实现信息提供能力，math 通过 interface 查询。

/// The three RA-09 slots of a retained exact-integer recurrence.
#[derive(Clone)]
pub struct ReduceShape {
    pub source: String,
    pub initial: crate::Expr,
    pub body: crate::Expr,
}

/// Prebound WHEN and result keys, in source slot order. The last WHEN is
/// omitted from the actual structure when its result is the ELSE.
pub type CaseArmKeys = Vec<(std::rc::Rc<crate::CanonicalKey>, std::rc::Rc<crate::CanonicalKey>)>;

/// 排序上下文 trait — 提供 HeapRef display_name 查询能力
pub trait SortContext {
    /// 查询 HeapRef 的 display_name（入口参数或命名状态的公开路径）
    ///
    /// - 有 display_name → 返回 Some(name)，用作主符号路径
    /// - 无 display_name → 返回 None，由排序规则使用 id 稳定字符串
    fn heap_display_name(&self, heap_id: i64) -> Option<String>;

    /// Public paths of already acquired, named environment observations.
    fn external_names(&self) -> Option<&std::collections::BTreeMap<u64, String>> {
        None
    }

    /// Published ordinals, which do not by themselves prove availability.
    fn temporal_aliases(&self) -> Option<&std::collections::BTreeMap<u64, usize>> {
        None
    }

    /// A selected member bound in the current YIELD, independent of
    /// publication handles (which also index unrelated REDUCE nodes).
    fn choice_slots(&self) -> Option<&std::collections::BTreeMap<u64, usize>> { None }

    /// Resolve a CASE prepared in this lexical scope. Scope construction
    /// already proved availability; key construction does not prove it again.
    fn case_arm_keys(&self, _case: &crate::Expr) -> Option<&CaseArmKeys> { None }

    fn has_case_arm_keys(&self) -> bool { false }

    /// Retained evaluation handles lower to this structure for comparison.
    fn reduce_shapes(&self) -> Option<&std::collections::BTreeMap<u64, ReduceShape>> { None }

    /// Body parameters are lexical slots only inside a recurrence's YIELD.
    fn in_reduce_body(&self) -> bool { false }

    /// Explicit evidence that the binding is acquired on every path.
    fn temporal_is_total(&self, _id: u64) -> bool { false }
}

/// 默认空上下文 — 所有 heap_display_name 返回 None
pub struct DefaultSortContext;

impl SortContext for DefaultSortContext {
    fn heap_display_name(&self, _heap_id: i64) -> Option<String> {
        None
    }
}


/// A recurrence YIELD with its own accumulator and member slots.
pub struct ReduceBodyContext;
impl ReduceBodyContext {
    // These qualified internal atoms cannot be Java parameter declarations.
    const BINDINGS: [(&str, &str); 2] = [("$reduce.acc", "acc"), ("$reduce.item", "item")];

    pub fn binding(slot: usize) -> crate::Expr {
        crate::builder::param(Self::BINDINGS[slot].0, crate::ScalarSymbolType::Integer)
    }

    pub fn resolve(atom: &str) -> Option<(usize, &'static str)> {
        Self::BINDINGS.iter().enumerate().find_map(|(slot, &(name, display))|
            (name == atom).then_some((slot, display)))
    }
}
impl SortContext for ReduceBodyContext {
    fn heap_display_name(&self, _heap_id: i64) -> Option<String> { None }
    fn in_reduce_body(&self) -> bool { true }
}
