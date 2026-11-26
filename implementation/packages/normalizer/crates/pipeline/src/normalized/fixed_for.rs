//! An induction variable with two literal endpoints has a closed integer trip count.
use simulator_parser::{
    JavaBinOp as B, JavaExpr as E, JavaLit, JavaStatement as S, JavaType, JavaUnaryOp as U,
};

pub(crate) struct FixedFor<'a> {
    pub counter: &'a str,
    pub start: i128,
    pub step: i128,
    pub count: i128,
    pub assigns_outer: bool,
}

fn literal(expr: &E) -> Option<i128> {
    match expr {
        E::Lit(JavaLit::Int(n)) => Some(i128::from(*n)),
        E::Lit(JavaLit::Long(n)) => Some(i128::from(*n)),
        E::UnaryOp {
            op: U::Neg,
            operand,
        } => literal(operand).map(|n| -n),
        _ => None,
    }
}

pub(super) fn assignment_init(init: Option<&S>) -> Option<(&str, &E, &E)> {
    let S::ExprStmt(E::Assign { target, value, is_compound: false }) = init? else {
        return None;
    };
    let E::Ident(name) = target.as_ref() else { return None; };
    Some((name, target, value))
}

pub(super) fn may_write_counter(body: &[S], counter: &str) -> bool {
    body.iter().any(|stmt| match stmt {
        S::Assign { target: E::Ident(name), .. } => name == counter,
        S::For { init, body, .. } => assignment_init(init.as_deref())
            .is_some_and(|(name, ..)| name == counter) || may_write_counter(body, counter),
        S::Block(body) | S::ForEach { body, .. } =>
            may_write_counter(body, counter),
        S::If { then_block, else_block, .. } => may_write_counter(then_block, counter)
            || else_block.as_ref().is_some_and(|body| may_write_counter(body, counter)),
        S::Try { body, catches, finally_block } => may_write_counter(body, counter)
            || catches.iter().any(|handler| may_write_counter(handler.body(), counter))
            || finally_block.as_ref().is_some_and(|body| may_write_counter(body, counter)),
        // Directives carry their own writes, not the Java lvalue above.
        S::LocalIr(_) => true,
        _ => false,
    })
}

impl<'a> FixedFor<'a> {
    pub fn from_header(init: Option<&'a S>, cond: Option<&E>, update: Option<&E>) -> Option<Self> {
        let (name, value, assigns_outer) = match init? {
            S::VarDecl {
                java_type: JavaType::Primitive(ty),
                name,
                initializer: Some(value),
                ..
            } if matches!(ty.as_str(), "int" | "long") => (name.as_str(), value, false),
            init => {
                let (name, _, value) = assignment_init(Some(init))?;
                (name, value, true)
            }
        };
        let E::BinOp { op, left, right } = cond? else {
            return None;
        };
        let E::UnaryOp {
            op: change,
            operand,
        } = update?
        else {
            return None;
        };
        if !matches!(left.as_ref(),E::Ident(key) if key==name)
            || !matches!(operand.as_ref(),E::Ident(key) if key==name)
        {
            return None;
        }
        let start = literal(value)?;
        let end = literal(right)?;
        let (step, extra) = match (op, change) {
            (B::Lt, U::PostInc | U::PreInc) => (1, 0),
            (B::Le, U::PostInc | U::PreInc) => (1, 1),
            (B::Gt, U::PostDec | U::PreDec) => (-1, 0),
            (B::Ge, U::PostDec | U::PreDec) => (-1, 1),
            _ => return None,
        };
        Some(Self {
            counter: name,
            start,
            step,
            count: ((end - start) * step + extra).max(0),
            assigns_outer,
        })
    }
}

/// A rectangular nest of unit-increment loops with independent exact
/// integer bounds and one constant update to a separate local accumulator.
pub(crate) struct RectangularCounter<'a> {
    pub bounds: Vec<(&'a str, bool)>,
    pub target: &'a str,
    pub increment: i64,
}
impl<'a> RectangularCounter<'a> {
    pub fn from_for(mut statement: &'a S) -> Option<Self> {
        let mut bounds = Vec::new();
        let mut counters = std::collections::BTreeSet::<&str>::new();
        while let S::For { init, cond, update, body } = statement {
            let S::VarDecl { java_type, name, initializer: Some(start), .. } = init.as_deref()? else { return None; };
            if !super::integral(java_type) || literal(start)? != 0 || !counters.insert(name) { return None; }
            let E::BinOp { op, left, right } = cond.as_ref()? else { return None; };
            let E::Ident(end) = right.as_ref() else { return None; };
            if !matches!(left.as_ref(), E::Ident(counter) if counter == name) || !matches!(op, B::Lt | B::Le) {
                return None;
            }
            let E::UnaryOp { op: U::PreInc | U::PostInc, operand } = update.as_ref()? else { return None; };
            if !matches!(operand.as_ref(), E::Ident(counter) if counter == name) { return None; }
            bounds.push((end.as_str(), *op == B::Le));
            let [inner] = body.as_slice() else { return None; };
            statement = inner;
        }
        let S::Assign { target: E::Ident(target), value: E::BinOp { op: B::Add, left, right }, .. } = statement else { return None; };
        if bounds.len() < 2 || counters.contains(target.as_str())
            || bounds.iter().any(|(end, _)| counters.contains(end) || end == target)
        {
            return None;
        }
        let increment = if matches!(left.as_ref(), E::Ident(name) if name == target) { literal(right)? }
            else if matches!(right.as_ref(), E::Ident(name) if name == target) { literal(left)? }
            else { return None; };
        Some(Self { bounds, target, increment: increment.try_into().ok()? })
    }
}
