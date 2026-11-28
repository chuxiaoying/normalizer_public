//! Internal graph projection for the lexical CHOOSE form of RA-09.
use math::Expr;
use std::{collections::{BTreeMap, BTreeSet, HashMap}, rc::Rc};

#[derive(Clone)]
pub(crate) enum Operand {
    Integer(math::Integer),
    Parameter(String),
    Acquired(String),
    Prior(usize),
    Computed { value: Expr, dependencies: Vec<usize> },
}

#[derive(Clone)]
pub(crate) enum Slot {
    Choice { members: String, empty: Operand, guard: Expr, shape: ChoiceShape },
}

#[derive(Clone)]
pub(crate) enum ChoiceOuter {
    Parameter { name: String, kind: String },
    Expression(Expr),
}

impl ChoiceOuter {
    pub(super) fn key(&self, ctx: &dyn math::SortContext) -> Rc<math::CanonicalKey> {
        match self {
            Self::Parameter { name, kind } => math::named_parameter_key(name, kind),
            Self::Expression(value) => math::canonical_order_key(value, ctx),
        }
    }
}

#[derive(Clone)]
pub(crate) struct ChoiceShape {
    pub(crate) source: ChoiceOuter,
    pub(crate) fallback: ChoiceOuter,
}

/// A rendering uses one fixed set of named inputs and external observations.
pub(super) struct ScopedPlans {
    pub(super) external: BTreeMap<u64, math::ObservationIdentity>,
    pub(super) plans: std::cell::RefCell<HashMap<ValueLocation, math::ValuePlan>>,
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub(super) struct ValueLocation {
    pub(super) value: Expr,
    pub(super) domain: Expr,
    pub(super) members: Vec<usize>,
}

#[derive(Default)]
pub(super) struct ValueKeys {
    pub(super) prefix: Vec<math::ChoiceOperands>,
    pub(super) lexical: BTreeMap<u64, usize>,
    pub(super) cases: HashMap<Expr, math::CaseArmKeys>,
}

#[derive(Default)]
pub(super) struct Scope {
    pub(super) slots: Rc<[Slot]>,
    pub(super) active: Option<Expr>,
    pub(super) members: BTreeMap<usize, String>,
    pub(super) member_order: Vec<usize>,
}

pub(super) fn add_dependencies(slots: &[Slot], slot: usize, needed: &mut BTreeSet<usize>) {
    if needed.insert(slot) {
        match &slots[slot] {
            Slot::Choice { empty: Operand::Prior(dep), .. } => add_dependencies(slots, *dep, needed),
            Slot::Choice { empty: Operand::Computed { dependencies, .. }, .. } => {
                for &dep in dependencies { add_dependencies(slots, dep, needed); }
            }
            _ => {}
        }
    }
}
