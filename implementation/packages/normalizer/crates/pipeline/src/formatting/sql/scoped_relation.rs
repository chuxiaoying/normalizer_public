//! Internal graph projection for the lexical REDUCE and CHOOSE forms of RA-09.
use math::Expr;
use std::{collections::{BTreeMap, BTreeSet}, rc::Rc};

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Operand {
    Integer(math::Integer),
    Parameter(String),
    Acquired(String),
    Prior(usize),
}

#[derive(Clone)]
pub(crate) enum Slot {
    Fold { source: String, initial: Operand, body: String },
    Choice { members: String, empty: Operand, guard: Expr },
}

#[derive(Default)]
pub(super) struct Scope {
    pub(super) slots: Rc<[Slot]>,
    pub(super) active: Option<Expr>,
    pub(super) members: BTreeMap<usize, String>,
}

impl Scope {
    pub(super) fn operand(&self, operand: &Operand) -> String {
        match operand {
            Operand::Integer(n) => n.to_string(),
            Operand::Parameter(name) | Operand::Acquired(name) => name.clone(),
            Operand::Prior(slot) => self.slot(*slot),
        }
    }

    pub(super) fn slot(&self, slot: usize) -> String {
        match &self.slots[slot] {
            Slot::Fold { source, initial, body } =>
                format!("REDUCE({source} FROM {} AS (acc, item) YIELD {body})", self.operand(initial)),
            Slot::Choice { .. } => self.members.get(&slot)
                .expect("choice must be bound in its certified acquisition domain").clone(),
        }
    }
}

pub(super) fn add_dependencies(slots: &[Slot], slot: usize, needed: &mut BTreeSet<usize>) {
    if needed.insert(slot) {
        match &slots[slot] {
            Slot::Fold { initial: Operand::Prior(dep), .. } | Slot::Choice { empty: Operand::Prior(dep), .. } =>
                add_dependencies(slots, *dep, needed),
            _ => {}
        }
    }
}
