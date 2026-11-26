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
    pub declarations: Option<&'a [S]>,
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
pub(super) fn closed_assignment_counter(body: &[S], counter: &str, mutable_counter: bool) -> bool {
    body.iter().all(|statement| match statement {
        S::Assign { target: E::Ident(name), value, .. } => name != counter
            || mutable_counter && literal(value).is_some(),
        S::Return(_) => true,
        S::Block(body) => closed_assignment_counter(body, counter, mutable_counter),
        S::If { then_block, else_block, .. } =>
            closed_assignment_counter(then_block, counter, mutable_counter)
                && else_block.as_deref().is_none_or(|body| closed_assignment_counter(body, counter, mutable_counter)),
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
        let eligible_type = |ty: &JavaType|
            matches!(ty, JavaType::Primitive(ty) if matches!(ty.as_str(), "int" | "long"));
        let (name, value, assigns_outer, declarations) = if let Some((name, ty, value)) = declares_counter(init) {
            if !eligible_type(ty) { return None; }
            (name, value, false, None)
        } else if let S::Block(declarations) = init {
            let E::UnaryOp { operand, .. } = update? else { return None; };
            let E::Ident(induction_name) = operand.as_ref() else { return None; };
            let mut induction = None;
            for declaration in declarations {
                let (name, ty, value) = declares_counter(declaration)?;
                if !eligible_type(ty) { return None; }
                if name == induction_name {
                    if induction.replace((name, value)).is_some() { return None; }
                } else if literal(value).is_none() && !matches!(value, E::Ident(_)) {
                    // Extra declarators may use an earlier local, but may not
                    // reassign the counter or have other execution effects.
                    return None;
                }
            }
            let (name, value) = induction?;
            (name, value, false, Some(declarations.as_slice()))
        } else {
            let (name, _, value) = assignment_init(init)?;
            (name, value, true, None)
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
            declarations,
        })
    }
}

/// A finite sequence of native loop prefixes. The header admits only one
/// monotone integer induction; each body control decision is evaluated on
/// its concrete counter value, without treating break/continue as both
/// fallthrough and exit. Unknown header bounds still require a proved break.
pub(crate) struct BoundedControlFor<'a> {
    pub counter: &'a str,
    pub bound: Option<&'a str>,
    pub inclusive: bool,
    pub iterations: Vec<(i128, Vec<&'a S>)>,
}

const PREFIX_LIMIT: usize = 256;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Transfer {
    Fallthrough,
    Next,
    Exit,
}

fn inductive_guard(expr: &E, counter: &str, index: i128) -> Option<bool> {
    use simulator_parser::JavaLit;
    let value = |expr: &E| if is_counter(expr, counter) {
        Some(index)
    } else {
        literal(expr)
    };
    match expr {
        E::Lit(JavaLit::Bool(b)) => Some(*b),
        E::UnaryOp { op: U::Not, operand } => Some(!inductive_guard(operand, counter, index)?),
        E::BinOp { op: B::And | B::Or, left, right } => {
            let E::BinOp { op, .. } = expr else { unreachable!() };
            let left = inductive_guard(left, counter, index)?;
            let right = inductive_guard(right, counter, index)?;
            Some(if *op == B::And { left && right } else { left || right })
        }
        E::BinOp { op, left, right } => {
            let (left, right) = (value(left)?, value(right)?);
            Some(match op {
                B::Eq => left == right,
                B::Ne => left != right,
                B::Lt => left < right,
                B::Le => left <= right,
                B::Gt => left > right,
                B::Ge => left >= right,
                _ => return None,
            })
        }
        _ => None,
    }
}

fn closed_integer_value(expr: &E) -> bool {
    literal(expr).is_some() || match expr {
        E::Ident(_) => true,
        E::BinOp { op: B::Add | B::Sub | B::Mul, left, right } =>
            closed_integer_value(left) && closed_integer_value(right),
        _ => false,
    }
}

/// Check every source path, not only the paths visited during a prefix. No
/// unknown expression can execute a call, create an abrupt operation, or
/// mutate the bound regardless of which header domain is selected.
fn closed_loop_control(body: &[S], counter: &str, bound: Option<&str>) -> Option<bool> {
    let mut has_transfer = false;
    for statement in body {
        has_transfer |= match statement {
            S::LoopBreak | S::LoopContinue => true,
            S::Block(body) => closed_loop_control(body, counter, bound)?,
            S::If { cond, then_block, else_block } => {
                inductive_guard(cond, counter, 0)?;
                closed_loop_control(then_block, counter, bound)?
                    | closed_loop_control(else_block.as_deref().unwrap_or_default(), counter, bound)?
            }
            S::Assign { target: E::Ident(name), value, .. }
                if name != counter && bound != Some(name.as_str()) && closed_integer_value(value) => false,
            _ => return None,
        };
    }
    Some(has_transfer)
}

fn loop_prefix<'a>(body: &'a [S], counter: &str, index: i128, statements: &mut Vec<&'a S>) -> Transfer {
    for statement in body {
        let outcome = match statement {
            S::LoopBreak => Transfer::Exit,
            S::LoopContinue => Transfer::Next,
            S::Block(body) => loop_prefix(body, counter, index, statements),
            S::If { cond, then_block, else_block } => {
                let selected = if inductive_guard(cond, counter, index).expect("closed induction guard") {
                    then_block
                } else {
                    else_block.as_deref().unwrap_or_default()
                };
                loop_prefix(selected, counter, index, statements)
            }
            S::Assign { .. } => {
                statements.push(statement);
                Transfer::Fallthrough
            }
            _ => unreachable!("closed native loop prefix"),
        };
        if outcome != Transfer::Fallthrough { return outcome; }
    }
    Transfer::Fallthrough
}

impl<'a> BoundedControlFor<'a> {
    pub fn from_for(statement: &'a S) -> Option<Self> {
        let S::For { init, cond, update, body } = statement else { return None; };
        let (counter, ty, initial) = declares_counter(init.as_deref()?)?;
        let JavaType::Primitive(host) = ty else { return None; };
        let fits = |value| match host.as_str() {
            "int" => i32::try_from(value).is_ok(),
            "long" => i64::try_from(value).is_ok(),
            _ => false,
        };
        let start = literal(initial)?;
        if !fits(start) { return None; }
        let (end, 1, inclusive) = induction(counter, cond.as_ref(), update.as_ref())? else { return None; };
        let (bound, literal_end) = match end {
            E::Ident(name) if name != counter => (Some(name.as_str()), None),
            value => (None, Some(literal(value)?)),
        };
        if !closed_loop_control(body, counter, bound)? { return None; }
        let mut iterations = Vec::new();
        let mut work = 0usize;
        for offset in 0..=PREFIX_LIMIT {
            let index = start + offset as i128;
            if literal_end.is_some_and(|end| index > end || (index == end && !inclusive)) {
                return Some(Self { counter, bound, inclusive, iterations });
            }
            if offset == PREFIX_LIMIT || !fits(index) { return None; }
            let mut statements = Vec::new();
            let transfer = loop_prefix(body, counter, index, &mut statements);
            work = work.checked_add(statements.len())?;
            if work > 4096 { return None; }
            iterations.push((index, statements));
            if transfer == Transfer::Exit {
                return Some(Self { counter, bound, inclusive, iterations });
            }
        }
        None
    }
}

/// Closed scalar induction on a private local object's field. The source
/// header certifies finite motion; execution must independently bind all
/// three receiver names to the same fresh object.
pub(crate) struct FixedFieldFor<'a> {
    pub receivers: [&'a str; 3],
    pub field: &'a str,
    pub start: i128,
    pub step: i128,
    pub count: i128,
}

impl<'a> FixedFieldFor<'a> {
    pub fn from_for(statement: &'a S) -> Option<Self> {
        fn field(expr: &E) -> Option<(&str, &str)> {
            let E::FieldAccess { target, field } = expr else { return None; };
            let E::Ident(receiver) = target.as_ref() else { return None; };
            Some((receiver, field))
        }
        let S::For { init, cond, update, body } = statement else { return None; };
        let S::ExprStmt(E::Assign { target, value, is_compound: false }) = init.as_deref()? else { return None; };
        let (start_root, member) = field(target)?;
        let E::BinOp { op, left, right } = cond.as_ref()? else { return None; };
        let ((test_root, tested), op, end) = if let Some(field) = field(left) {
            (field, *op, literal(right)?)
        } else {
            let swap = match op { B::Lt => B::Gt, B::Le => B::Ge, B::Gt => B::Lt, B::Ge => B::Le, _ => return None };
            (field(right)?, swap, literal(left)?)
        };
        let E::UnaryOp { op: change, operand } = update.as_ref()? else { return None; };
        let (step_root, stepped) = field(operand)?;
        if member != tested || member != stepped { return None; }
        let (step, inclusive) = match (op, change) {
            (B::Lt, U::PostInc | U::PreInc) => (1, false),
            (B::Le, U::PostInc | U::PreInc) => (1, true),
            (B::Gt, U::PostDec | U::PreDec) => (-1, false),
            (B::Ge, U::PostDec | U::PreDec) => (-1, true),
            _ => return None,
        };
        let receivers = [start_root, test_root, step_root];
        fn integer_scalar(expr: &E, receivers: [&str; 3], member: &str) -> bool {
            if literal(expr).is_some() { return true; }
            match expr {
                E::Ident(name) => !receivers.contains(&name.as_str()),
                E::FieldAccess { .. } => field(expr).is_some_and(|(root, access)|
                    receivers.contains(&root) && access == member),
                E::BinOp { op: B::Add | B::Sub | B::Mul, left, right } =>
                    integer_scalar(left, receivers, member) && integer_scalar(right, receivers, member),
                _ => false,
            }
        }
        // No call, receiver assignment, field write, or nested loop can
        // mutate this counter behind the verified header update.
        if !body.iter().all(|stmt| matches!(stmt,
            S::Assign { target: E::Ident(name), value, .. }
                if !receivers.contains(&name.as_str()) && integer_scalar(value, receivers, member)
        )) { return None; }
        let start = literal(value)?;
        let count = ((end - start) * step + i128::from(inclusive)).max(0);
        (count <= 256).then_some(Self { receivers, field: member, start, step, count })
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
            if !super::integral(ty) || literal(start)? != 0 { return None; }
            // Lexical binding uniqueness is checked once by source
            // admission. This set is only for proving independence.
            counters.insert(name);
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

#[cfg(test)]
mod rectangular_proof_contract {
    use super::*;
    use simulator_parser::{ParseOptions, parse};

    fn with_loop(body: &str, inspect: impl FnOnce(&S)) {
        let source = format!(
            "class Grid {{ static int count(int rows, int columns) {{ int total = 0; {body} return total; }} }}"
        );
        let program = parse(&source, "Grid.java", &ParseOptions::default()).unwrap();
        let method = &program.find_class("Grid").unwrap().methods()[0];
        let [_, loop_stmt @ S::For { .. }, _] = method.body().unwrap() else {
            panic!("the test source must contain one outer for loop");
        };
        inspect(loop_stmt);
    }

    #[test]
    fn two_independent_domains_preserve_inclusive_upper_bounds_and_increment() {
        with_loop("for (int i = 0; i < rows; i++) {
            for (int j = 0; columns >= j; j++) { total = total + 2; }
        }", |statement| {
            let rectangle = RectangularCounter::from_for(statement).unwrap();
            assert_eq!(rectangle.bounds, vec![("rows", false), ("columns", true)]);
            assert_eq!(rectangle.target, "total");
            assert_eq!(rectangle.increment, 2);
        });
    }

    #[test]
    fn triangular_and_mutating_shapes_cannot_borrow_the_rectangular_proof() {
        for body in [
            "for (int i = 0; i < rows; i++) { for (int j = 0; j < i; j++) { total = total + 1; } }",
            "for (int i = 0; i < rows; i++) { for (int j = 0; j < columns; j++) { i = i + 1; } }",
            "for (int i = 0; i < rows; i++) { for (int j = 0; j < columns; j++) { total = total - 1; } }",
            "for (double i = 0; i < rows; i++) { for (int j = 0; j < columns; j++) { total = total + 1; } }",
            "for (int i = 0; i < rows; i++) { total = total + 1; }",
            "for (int i = 0; i < rows; i++) { total = total + 1; for (int j = 0; j < columns; j++) { total = total + 1; } }",
        ] {
            with_loop(body, |statement| assert!(RectangularCounter::from_for(statement).is_none()));
        }
    }

    #[test]
    fn repeated_local_names_are_rejected_by_source_binding_admission() {
        let program = parse(
            "class Grid { static int count(int rows, int columns) {
                int total = 0;
                for (int i = 0; i < rows; i++) {
                    for (int i = 0; i < columns; i++) { total = total + 1; }
                }
                return total;
            } }",
            "Grid.java",
            &ParseOptions::default(),
        ).unwrap();
        let error = super::super::validate(&program, "Grid.count").err()
            .expect("overlapping local declarations are illegal Java");
        assert!(error.contains("[NORM-SOURCE-NAME]") && error.contains("duplicate local declaration i"));
    }
}
