//! Business SELECT output for native relational aggregates.

use math::{
    domains::relation::{self as r, RelationExpr},
    BinOpKind, Expr, SymVarOrigin,
};

pub(super) fn render(expr: &Expr, ctx: &mut crate::FormatCtx) -> Option<String> {
    let (kind, relation, Some(value), default) = expr.as_relation_aggregate()? else {
        return None;
    };
    if kind == math::RelationAggregateKind::Sum && value == &math::builder::lit_int(1) {
        if let Some((table, keys)) = crate::kernel_input::business_relation::grouped_table(relation) {
            let keys = keys.iter().map(|k| crate::format_expr_with_ctx(k, ctx)).collect::<Vec<_>>().join(", ");
            return Some(format!("COALESCE(\n  SUM(\n    SELECT 1\n    FROM (\n      SELECT {keys}\n      FROM {table}\n      GROUP BY {keys}\n    ) AS groups\n  ),\n  0\n)"));
        }
    }
    if kind == math::RelationAggregateKind::Sum {
        if let Some(mut join) = crate::kernel_input::business_relation::inner_join_count(relation, value) {
            let rank = |expr: &Expr| {
                let mut left = false;
                let mut right = false;
                expr.walk(&mut |node| {
                    if let Some((alias, _)) = node.as_row_value().and_then(|p| p.split_once('.')) {
                        left |= alias == join.left.1;
                        right |= alias == join.right.1;
                    }
                });
                if left { 0 } else if right { 1 } else { 2 }
            };
            let fmt_predicate = |expr: &Expr, ctx: &mut crate::FormatCtx| {
                if let Some((op, l, r)) = expr.as_binop() {
                    let flip = rank(r) < rank(l);
                    let op = match (op, flip) {
                        (BinOpKind::Eq, _) => "==", (BinOpKind::Ne, _) => "!=",
                        (BinOpKind::Gt, false) | (BinOpKind::Lt, true) => ">",
                        (BinOpKind::Ge, false) | (BinOpKind::Le, true) => ">=",
                        (BinOpKind::Lt, false) | (BinOpKind::Gt, true) => "<",
                        (BinOpKind::Le, false) | (BinOpKind::Ge, true) => "<=",
                        _ => return crate::format_expr_with_ctx(expr, ctx),
                    };
                    let (l,r) = if flip { (r,l) } else { (l,r) };
                    return format!("{} {op} {}", crate::format_expr_with_ctx(l,ctx), crate::format_expr_with_ctx(r,ctx));
                }
                crate::format_expr_with_ctx(expr,ctx)
            };
            let ((left, left_alias), (right, right_alias)) = (join.left, join.right);
            let on = fmt_predicate(join.on, ctx);
            join.filters.sort_by_key(|f| (rank(f), math::canonical_order_key(f, &math::DefaultSortContext)));
            let mut out = format!("COALESCE(\n  SUM(\n    SELECT 1\n    FROM {left} AS {left_alias}\n    JOIN {right} AS {right_alias} ON {on}");
            for (i, f) in join.filters.into_iter().enumerate() {
                out.push_str(if i == 0 { "\n    WHERE " } else { "\n      AND " });
                out.push_str(&fmt_predicate(f, ctx));
            }
            out.push_str("\n  ),\n  0\n)");
            return Some(out);
        }
    }
    let (mut input, keys) = r::order_components(relation).unwrap_or((relation, &[]));
    let distinct = match input {
        RelationExpr::Distinct(inner) if kind == math::RelationAggregateKind::Count => { input = inner; true }
        _ => false,
    };
    let mut predicates = Vec::new();
    while let RelationExpr::Select {
        predicate,
        input: inner,
    } = input
    {
        fn flat(expr: &Expr, out: &mut Vec<Expr>) {
            if let Some((BinOpKind::And, a, b)) = expr.as_binop() {
                flat(a, out);
                flat(b, out)
            } else {
                out.push(expr.clone())
            }
        }
        flat(predicate, &mut predicates);
        input = inner;
    }
    let scalar_array = matches!(input, RelationExpr::Base {name, ..} if ctx.scalar_arrays.contains(name));
    // Math can factor a guarded nth(0) into FirstValue. Preserve the single
    // canonical positional surface for a directly bound scalar array.
    if scalar_array && kind == math::RelationAggregateKind::FirstValue
        && predicates.is_empty() && keys.is_empty() && value.as_row_value() == Some("value")
    {
        let RelationExpr::Base { name, .. } = input else { unreachable!() };
        let array = super::array_name(name, ctx).to_owned();
        let default = default?;
        if default.is_null() && ctx.array_exit_guard.as_ref().is_some_and(|path|
            math::builder::path_implies(path, &math::normalize(r::relation_exists(relation.clone())))) {
            return Some(format!("{array}[0]"));
        }
        let empty = crate::format_expr_with_ctx(default, ctx);
        return Some(format!("CASE WHEN LENGTH({array}) == 0 THEN {empty} ELSE {array}[0] END"));
    }
    let record_array = matches!(input, RelationExpr::Base { name, .. } if ctx.record_arrays.contains(name));
    let nested = matches!(input, RelationExpr::Base {name, ..} if ctx.nested_arrays.contains(name));
    let correlated = matches!(input, RelationExpr::Vector {value, ..} if value.as_row_value().is_some());
    let from = match input {
        RelationExpr::Base { name, alias } if record_array || nested => format!("{} AS {alias}", super::format_parameter_name(name, ctx)),
        RelationExpr::Base { name, .. } if scalar_array =>
            super::array_name(name, ctx).to_owned(),
        RelationExpr::Base { name, .. } => name.clone(),
        RelationExpr::Vector { value, alias } => {
            format!("{} AS {alias}", crate::format_expr_with_ctx(value, ctx))
        }
        _ => return None,
    };
    let fmt = |e: &Expr, ctx: &mut crate::FormatCtx| {
        if let Some(path) = e.as_row_value() {
            return path.to_owned();
        }
        crate::format_expr_with_ctx(e, ctx)
    };
    fn class(e: &Expr) -> u8 {
        let mut rank = 2;
        e.walk(&mut |node| match node.as_sym_var() {
            Some(SymVarOrigin::Parameter { .. }) => rank = 0,
            Some(SymVarOrigin::External { .. }) => rank = rank.min(1),
            _ => {}
        });
        if rank == 2 {
            if let Some((_, l, r)) = e.as_binop() {
                if l.as_lit().is_some() || r.as_lit().is_some() {
                    rank = 3;
                }
                if [l, r]
                    .iter()
                    .any(|v| matches!(v.as_lit(), Some(math::Lit::Str(_))))
                {
                    rank = 4;
                }
            }
        }
        rank
    }
    predicates.sort_by_key(|e| {
        (
            class(e),
            math::canonical_order_key(e, &math::DefaultSortContext),
        )
    });
    let mut select = vec![
        format!("SELECT {}", fmt(value, ctx)),
        format!("FROM {from}"),
    ];
    let predicate_text = |e: &Expr, ctx: &mut crate::FormatCtx| {
        if let Some((op, l, r)) = e.as_binop() {
            if matches!(op, BinOpKind::Eq | BinOpKind::Ne) {
                let column = if r.is_null() && l.as_row_value().is_some() { Some(l) }
                    else if l.is_null() && r.as_row_value().is_some() { Some(r) } else { None };
                if let Some(column) = column {
                    let predicate = if op == BinOpKind::Eq { "IS NULL" } else { "IS NOT NULL" };
                    return format!("{} {predicate}", fmt(column, ctx));
                }
            }
            // Keep table columns on the left against simple input/literal operands.
            // A lower external threshold is naturally displayed as threshold <= column.
            let has_external = |value: &Expr| {
                let mut found = false;
                value.walk(&mut |part| {
                    found |= matches!(part.as_sym_var(), Some(SymVarOrigin::External { .. }));
                });
                found
            };
            let flip = l.as_row_value().is_none() && r.as_row_value().is_some() && !has_external(l);
            let reverse = |o| match o {
                BinOpKind::Gt => BinOpKind::Lt,
                BinOpKind::Lt => BinOpKind::Gt,
                BinOpKind::Ge => BinOpKind::Le,
                BinOpKind::Le => BinOpKind::Ge,
                k => k,
            };
            let (a, mut op, z) = if flip {
                (r, reverse(op), l)
            } else {
                (l, op, r)
            };
            let (a, z) = if matches!(op, BinOpKind::Ge | BinOpKind::Gt) && has_external(z) {
                op = reverse(op);
                (z, a)
            } else {
                (a, z)
            };
            if let Some(text) = match op {
                BinOpKind::Eq => Some("=="),
                BinOpKind::Ne => Some("!="),
                BinOpKind::Gt => Some(">"),
                BinOpKind::Ge => Some(">="),
                BinOpKind::Lt => Some("<"),
                BinOpKind::Le => Some("<="),
                _ => None,
            } {
                return format!("{} {text} {}", fmt(a, ctx), fmt(z, ctx));
            }
        }
        fmt(e, ctx)
    };
    for (i, p) in predicates.iter().enumerate() {
        select.push(format!(
            "{} {}",
            if i == 0 { "WHERE" } else { "  AND" },
            predicate_text(p, ctx)
        ))
    }
    if !keys.is_empty() {
        select.push(format!(
            "ORDER BY {}",
            keys.iter()
                .map(|k| fmt(k, ctx))
                .collect::<Vec<_>>()
                .join(", ")
        ))
    }
    if scalar_array && kind == math::RelationAggregateKind::Count {
        let prefix = if distinct { "DISTINCT " } else { "" };
        let parts = select.join(" ");
        return Some(format!("COUNT({prefix}{})", parts.strip_prefix("SELECT ")?));
    }
    let (lead, default) = match kind {
        math::RelationAggregateKind::Sum => ("SUM(", "0".into()),
        math::RelationAggregateKind::FirstValue => {
            select.push("LIMIT 1".into());
            ("(", fmt(default?, ctx))
        }
        _ => return None,
    };
    if scalar_array || record_array || nested || correlated {
        return Some(format!("COALESCE({lead}{}), {default})", select.join(" ")));
    }
    Some(format!(
        "COALESCE(\n  {lead}\n    {}\n  ),\n  {default}\n)",
        select.join("\n    ")
    ))
}
