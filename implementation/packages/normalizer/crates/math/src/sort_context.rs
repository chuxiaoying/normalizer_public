//! SortContext trait — IoC 控制反转：math 要求排序参与者提供必要信息
//!
//! ## 设计
//!
//! math 不依赖执行堆。排序通过此接口读取有语义的绑定身份。

/// Prebound WHEN and result keys, in source slot order. The last WHEN is
/// omitted from the actual structure when its result is the ELSE.
pub type CaseArmKeys = Vec<(std::rc::Rc<crate::CanonicalKey>, std::rc::Rc<crate::CanonicalKey>)>;

/// Bound environment identity in one entry's comparison context. Dynamic
/// slots index the settled event ledger, not its separately numbered outputs.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ObservationIdentity {
    EntrySnapshot(Vec<String>),
    Receipt(usize),
    RuntimeObservation(usize),
    Internal(u64),
}

/// An acquired object can alias another object without sharing its binding.
#[derive(Clone)]
pub enum HeapBindingIdentity {
    Parameter(Vec<String>),
    EntrySnapshot(Vec<String>),
}

/// Supplies the bound identities used to compare an expression.
pub trait SortContext {
    /// Checked adapter aliases for immutable entry parameters. An alias is
    /// a direct read of the same P binding, not a field access on its name.
    fn captured_parameters(&self) -> Option<&std::collections::BTreeMap<String, String>> { None }

    fn heap_binding_identity(&self, _id: i64) -> Option<&HeapBindingIdentity> { None }

    /// Stable identities of already acquired environment observations.
    fn observation_identities(&self) -> Option<&std::collections::BTreeMap<u64, ObservationIdentity>> {
        None
    }

    /// Sorted exact failure tags of a closed completion acquired on every path.
    /// The caller may not infer this from a catch type or from a label.
    fn total_failure_classes(&self, _receipt: u64) -> Option<Vec<&str>> { None }

    /// A successful value of a nonthrowing occurrence reached on every
    /// path, not merely the name that would be rendered for its receipt.
    fn is_total_receipt_value(&self, _receipt: u64) -> bool { false }

    /// Published ordinals, which do not by themselves prove availability.
    fn temporal_aliases(&self) -> Option<&std::collections::BTreeMap<u64, usize>> {
        None
    }

    /// A selected member bound in the current YIELD, independent of
    /// publication handles.
    fn choice_slots(&self) -> Option<&std::collections::BTreeMap<u64, usize>> { None }

    /// Resolve a CASE prepared in this lexical scope. Scope construction
    /// already proved availability; key construction does not prove it again.
    fn case_arm_keys(&self, _case: &crate::Expr) -> Option<&CaseArmKeys> { None }

    fn has_case_arm_keys(&self) -> bool { false }

    /// Explicit evidence that the binding is acquired on every path.
    fn temporal_is_total(&self, _id: u64) -> bool { false }
}

/// Context for expressions with no acquired binding metadata.
pub struct DefaultSortContext;

impl SortContext for DefaultSortContext {}
