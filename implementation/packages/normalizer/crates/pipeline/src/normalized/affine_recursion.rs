//! A small total family of pure static integer recurrences. The source guard
//! proves decreasing distance from a boundary; the base cases prove the same
//! affine candidate for every residue reached before one full step.
use simulator_parser::{JavaBinOp as B, JavaExpr as E, JavaMethod, JavaStatement as S, JavaType};

pub(crate) struct AffineRecurrence {
    pub boundary: i128,
    pub base: i128,
    pub slope: i128,
}

pub(crate) fn prove(method: &JavaMethod) -> Option<AffineRecurrence> {
    let int = |ty: &JavaType| matches!(ty, JavaType::Primitive(name) if matches!(name.as_str(), "int" | "long"));
    if !method.is_static() || !int(method.return_type()) {
        return None;
    }
    let [(parameter, name)] = method.params() else {
        return None;
    };
    if !int(parameter) {
        return None;
    }
    let is_parameter = |expr: &E| matches!(expr, E::Ident(value) if value == name);
    let literal = |expr: &E| super::integer_token(expr).map(i128::from);
    let (first, rest) = method.body()?.split_first()?;
    let S::If {
        cond: E::BinOp {
            op: B::Le,
            left,
            right,
        },
        then_block,
        else_block: None,
    } = first
    else {
        return None;
    };
    if !is_parameter(left) {
        return None;
    }
    let boundary = literal(right)?;
    let [S::Return(Some(base))] = then_block.as_slice() else {
        return None;
    };
    let base = literal(base)?;
    let (last, explicit) = rest.split_last()?;
    let S::Return(Some(E::BinOp {
        op: B::Add,
        left,
        right,
    })) = last
    else {
        return None;
    };
    let E::MethodCall {
        target: None,
        method: callee,
        args,
    } = left.as_ref()
    else {
        return None;
    };
    if callee != method.name() {
        return None;
    }
    let [E::BinOp {
        op: B::Sub,
        left,
        right: step,
    }] = args.as_slice()
    else {
        return None;
    };
    if !is_parameter(left) {
        return None;
    }
    let step = literal(step)?;
    let increment = literal(right)?;
    if step <= 0 || increment % step != 0 {
        return None;
    }
    // Need precisely the step-1 adjacent positive base cases. The source
    // contains all of them, so proof work is bounded by the supplied AST.
    if i128::try_from(explicit.len()).ok()? + 1 != step {
        return None;
    }
    let slope = increment / step;
    let mut covered = std::collections::BTreeSet::new();
    for statement in explicit {
        let S::If {
            cond:
                E::BinOp {
                    op: B::Eq,
                    left,
                    right,
                },
            then_block,
            else_block: None,
        } = statement
        else {
            return None;
        };
        if !is_parameter(left) {
            return None;
        }
        let point = literal(right)?;
        let [S::Return(Some(value))] = then_block.as_slice() else {
            return None;
        };
        if point <= boundary
            || point >= boundary + step
            || !covered.insert(point)
            || literal(value)? != base + (point - boundary) * slope
        {
            return None;
        }
    }
    Some(AffineRecurrence {
        boundary,
        base,
        slope,
    })
}
