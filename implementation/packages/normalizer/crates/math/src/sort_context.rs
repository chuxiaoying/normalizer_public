//! SortContext trait — IoC 控制反转：math 要求排序参与者提供必要信息
//!
//! ## 设计
//!
//! math 不依赖执行堆。排序通过此接口读取有语义的绑定身份。

/// The three RA-09 slots of a retained exact-integer recurrence.
#[derive(Clone, PartialEq, Eq)]
pub struct ReduceShape {
    pub source: String,
    pub initial: crate::Expr,
    pub body: crate::Expr,
}

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
    fn heap_binding_identity(&self, _id: i64) -> Option<&HeapBindingIdentity> { None }

    /// Stable identities of already acquired environment observations.
    fn observation_identities(&self) -> Option<&std::collections::BTreeMap<u64, ObservationIdentity>> {
        None
    }

    /// Declared sole failure of a completion acquired on every path.
    /// The caller may not infer this from a catch type or from a label.
    fn total_single_failure_class(&self, _receipt: u64) -> Option<&str> { None }

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
    fn reduce_shape(&self, _id: u64) -> Option<&ReduceShape> { None }

    fn has_reduce_shapes(&self) -> bool { false }

    /// Body parameters are lexical slots only inside a recurrence's YIELD.
    fn in_reduce_body(&self) -> bool { false }

    /// Explicit evidence that the binding is acquired on every path.
    fn temporal_is_total(&self, _id: u64) -> bool { false }
}

/// Context for expressions with no acquired binding metadata.
pub struct DefaultSortContext;

impl SortContext for DefaultSortContext {}


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
    fn in_reduce_body(&self) -> bool { true }
}
