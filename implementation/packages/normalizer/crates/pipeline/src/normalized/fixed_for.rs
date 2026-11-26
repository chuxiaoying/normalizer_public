//! An induction variable with two literal endpoints has a closed integer trip count.
use simulator_parser::{
    JavaBinOp as B, JavaExpr as E, JavaStatement as S, JavaType, JavaUnaryOp as U,
};

pub(crate) struct FixedFor<'a> {
    pub counter: &'a str,
    pub start: i128,
    pub step: i128,
    pub count: i128,
    pub assigns_outer: bool,
}

fn literal(expr: &E) -> Option<i128> {
    if let E::UnaryOp { op: U::Neg, operand } = expr {
        literal(operand).map(|value| -value)
    } else {
        super::integer_token(expr).map(i128::from)
    }
}

pub(super) fn assignment_init(init: &S) -> Option<(&str, &E, &E)> {
    let S::ExprStmt(E::Assign { target, value, is_compound: false }) = init else {
        return None;
    };
    let E::Ident(name) = target.as_ref() else { return None; };
    Some((name, target, value))
}

// Assignment-initialized loops may assign other locals or return early.
// Every conditional arm must preserve the counter; other body shapes need
// their own proof rather than inheriting this one.
pub(super) fn preserves_assignment_counter(body: &[S], counter: &str) -> bool {
    body.iter().all(|statement| match statement {
        S::Assign { target: E::Ident(name), .. } => name != counter,
        S::Return(_) => true,
        S::Block(body) => preserves_assignment_counter(body, counter),
        S::If { then_block, else_block, .. } =>
            preserves_assignment_counter(then_block, counter)
                && else_block.as_deref().is_none_or(|body| preserves_assignment_counter(body, counter)),
        _ => false,
    })
}

fn declares_counter(statement: &S) -> Option<(&str, &JavaType, &E)> {
    let S::VarDecl { java_type, name, initializer: Some(value), .. } = statement else { return None; };
    Some((name, java_type, value))
}

fn is_counter(expr: &E, name: &str) -> bool {
    matches!(expr, E::Ident(key) if key == name)
}

// Return the bound, unit step and whether the bound is included.
fn induction<'a>(name: &str, cond: Option<&'a E>, update: Option<&E>) -> Option<(&'a E, i128, bool)> {
    let (E::BinOp { op, left, right }, E::UnaryOp { op: change, operand }) = (cond?, update?) else {
        return None;
    };
    if !is_counter(operand, name) {
        return None;
    }
    let (bound, op) = if is_counter(left, name) {
        (right.as_ref(), *op)
    } else if is_counter(right, name) {
        (left.as_ref(), match op {
            B::Lt => B::Gt, B::Le => B::Ge, B::Gt => B::Lt, B::Ge => B::Le,
            _ => return None,
        })
    } else {
        return None;
    };
    let (step, inclusive) = match (op, change) {
        (B::Lt, U::PostInc | U::PreInc) => (1, false),
        (B::Le, U::PostInc | U::PreInc) => (1, true),
        (B::Gt, U::PostDec | U::PreDec) => (-1, false),
        (B::Ge, U::PostDec | U::PreDec) => (-1, true),
        _ => return None,
    };
    Some((bound, step, inclusive))
}

impl<'a> FixedFor<'a> {
    pub fn from_header(init: Option<&'a S>, cond: Option<&E>, update: Option<&E>) -> Option<Self> {
        let init = init?;
        let (name, value, assigns_outer) = if let Some((name, _, value)) =
            declares_counter(init).filter(|(_, ty, _)| matches!(ty, JavaType::Primitive(ty) if matches!(ty.as_str(), "int" | "long")))
        {
            (name, value, false)
        } else {
            let (name, _, value) = assignment_init(init)?;
            (name, value, true)
        };
        let (bound, step, inclusive) = induction(name, cond, update)?;
        let start = literal(value)?;
        let end = literal(bound)?;
        Some(Self {
            counter: name,
            start,
            step,
            count: ((end - start) * step + i128::from(inclusive)).max(0),
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
            let (name, ty, start) = declares_counter(init.as_deref()?)?;
            if !super::integral(ty) || literal(start)? != 0 || !counters.insert(name) { return None; }
            let (E::Ident(end), 1, inclusive) =
                induction(name, cond.as_ref(), update.as_ref())?
            else { return None; };
            bounds.push((end.as_str(), inclusive));
            let [inner] = body.as_slice() else { return None; };
            statement = inner;
        }
        let S::Assign { target: E::Ident(target), value: E::BinOp { op: B::Add, left, right }, .. } = statement else { return None; };
        let offset = (bounds.len() >= 2 && !counters.contains(target.as_str())
            && bounds.iter().all(|(end, _)| !counters.contains(end) && end != target))
            .then(|| [(left, right), (right, left)]
                .into_iter()
                .find_map(|(acc, offset)| is_counter(acc, target).then_some(offset)))??;
        let increment = literal(offset)?;
        Some(Self { bounds, target, increment: increment.try_into().ok()? })
    }
}
