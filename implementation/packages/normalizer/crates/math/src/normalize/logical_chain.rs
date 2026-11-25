//! Structural AND/OR traversal for Boolean and first-match CASE rewrites.
//!
//! Keep terms in source order. Sorting, dropping terms and proving that a
//! rewrite preserves short-circuit evaluation belong to each caller.

use crate::{builder, expr::{BinOpKind, Expr}};

pub(super) fn flatten_and(expr: &Expr) -> Vec<Expr> {
    flatten(expr, BinOpKind::And)
}

pub(super) fn flatten_or(expr: &Expr) -> Vec<Expr> {
    flatten(expr, BinOpKind::Or)
}

fn flatten(expr: &Expr, target: BinOpKind) -> Vec<Expr> {
    fn visit(expr: &Expr, target: BinOpKind, terms: &mut Vec<Expr>) {
        if let Some((op, left, right)) = expr.as_binop() {
            if op == target {
                visit(left, target, terms);
                visit(right, target, terms);
                return;
            }
        }
        terms.push(expr.clone());
    }
    let mut terms = Vec::new();
    visit(expr, target, &mut terms);
    terms
}

pub(super) fn rebuild_and(terms: &[Expr]) -> Expr {
    rebuild(terms, true, builder::and)
}

pub(super) fn rebuild_or(terms: &[Expr]) -> Expr {
    rebuild(terms, false, builder::or)
}

/// Left-fold nonempty lists without inserting an extra identity operand.
/// Empty conjunctions are true; empty disjunctions are false.
fn rebuild(terms: &[Expr], identity: bool, join: fn(Expr, Expr) -> Expr) -> Expr {
    let mut terms = terms.iter().cloned();
    match terms.next() {
        Some(first) => terms.fold(first, join),
        None => builder::lit_bool(identity),
    }
}
