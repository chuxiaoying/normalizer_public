//! Internal graph projection for the lexical REDUCE and CHOOSE forms of RA-09.
use math::Expr;
use std::{collections::{BTreeMap, BTreeSet}, rc::Rc};

#[derive(Clone)]
pub(crate) enum Operand {
    Integer(math::Integer),
    Parameter(String),
    Acquired(String),
    Prior(usize),
    Arithmetic(Expr),
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

pub(super) fn add_dependencies(slots: &[Slot], slot: usize, needed: &mut BTreeSet<usize>) {
    if needed.insert(slot) {
        match &slots[slot] {
            Slot::Fold { initial: Operand::Prior(dep), .. } | Slot::Choice { empty: Operand::Prior(dep), .. } =>
                add_dependencies(slots, *dep, needed),
            _ => {}
        }
    }
}
