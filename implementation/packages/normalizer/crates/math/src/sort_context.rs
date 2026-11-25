//! SortContext trait — IoC 控制反转：math 要求排序参与者提供必要信息
//!
//! ## 设计
//!
//! math 层不依赖 pipeline/heap，但排序需要 HeapRef 的 display_name。
//! 通过 trait 让 pipeline 实现信息提供能力，math 通过 interface 查询。

/// 排序上下文 trait — 提供 HeapRef display_name 查询能力
pub trait SortContext {
    /// 查询 HeapRef 的 display_name（如 "$e_0.data"）
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
