//! Business SELECT output for native relational aggregates.

use math::{
    domains::relation::{self as r, RelationExpr},
    BinOpKind, Expr, SymVarOrigin,
};

// The SQL predicate owners share one comparison classification and reversal.
fn reversed_comparison(op: BinOpKind) -> Option<BinOpKind> {
    Some(match op {
        BinOpKind::Eq | BinOpKind::Ne => op,
        BinOpKind::Lt => BinOpKind::Gt,
        BinOpKind::Le => BinOpKind::Ge,
        BinOpKind::Gt => BinOpKind::Lt,
        BinOpKind::Ge => BinOpKind::Le,
        _ => return None,
    })
}

pub(super) fn render(expr: &Expr, ctx: &mut crate::FormatCtx) -> Option<String> {
    let (kind, relation, value, default) = expr.as_relation_aggregate()?;
    if kind == math::RelationAggregateKind::Count && value.is_none() && default.is_none() {
        if let Some(branches) = crate::kernel_input::business_relation::projected_union_columns(relation) {
            let distinct = if branches.len() == 1 { "DISTINCT " } else { "" };
            let selection = branches.into_iter().map(|(table, column)|
                format!("  SELECT {distinct}{column} FROM {table}")).collect::<Vec<_>>().join("\n  UNION\n");
            return Some(format!("SELECT COUNT(*) FROM (\n{selection}\n) AS accounts"));
        }
    }
    let one = math::builder::lit_int(1);
    let value = if kind == math::RelationAggregateKind::Exists {
        if !value.is_none_or(|v| v.as_bool() == Some(true)) { return None; }
        &one
    } else {
        value?
    };
    if kind == math::RelationAggregateKind::Sum {
        if value == &one {
            if let Some((table, keys)) = crate::kernel_input::business_relation::grouped_table(relation) {
                let keys = keys.iter().map(|k| crate::format_expr_with_ctx(k, ctx)).collect::<Vec<_>>().join(", ");
                return Some(format!("COALESCE(\n  SUM(\n    SELECT 1\n    FROM (\n      SELECT {keys}\n      FROM {table}\n      GROUP BY {keys}\n    ) AS groups\n  ),\n  0\n)"));
            }
        }
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
                let comparison = expr.as_binop().and_then(|(op, left, right)|
                    reversed_comparison(op).map(|reverse| (op, reverse, left, right)));
                let Some((op, reverse, left, right)) = comparison else {
                    return crate::format_expr_with_ctx(expr, ctx);
                };
                let (op, left, right) = if rank(right) < rank(left) {
                    (reverse, right, left)
                } else {
                    (op, left, right)
                };
                let token = super::binop_display_token(op);
                format!("{} {token} {}", crate::format_expr_with_ctx(left, ctx), crate::format_expr_with_ctx(right, ctx))
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
    let scalar_array = match input {
        RelationExpr::Base { name, .. } if ctx.scalar_arrays.contains(name) => Some(name),
        _ => None,
    };
    // Never convert the math layer's ordered first/null observation back
    // to an index. Explicit non-null fallbacks remain a separate contract.
    if let Some(name) = scalar_array.filter(|_| kind == math::RelationAggregateKind::FirstValue
        && predicates.is_empty() && keys.is_empty() && value.as_row_value() == Some("value"))
    {
        let array = super::array_name(name, ctx).to_owned();
        let default = default?;
        if default.is_null() {
            return Some(format!("FIRST(value FROM {array})"));
        }
        let empty = crate::format_expr_with_ctx(default, ctx);
        return Some(format!("CASE WHEN LENGTH({array}) == 0 THEN {empty} ELSE {array}[0] END"));
    }
    let record_array = matches!(input, RelationExpr::Base { name, .. } if ctx.record_arrays.contains(name));
    let nested = matches!(input, RelationExpr::Base {name, ..} if ctx.nested_arrays.contains(name));
    let correlated = matches!(input, RelationExpr::Vector {value, ..} if value.as_row_value().is_some());
    let from = match input {
        RelationExpr::Base { name, alias } if record_array || nested => format!("{name} AS {alias}"),
        RelationExpr::Base { name, .. } if scalar_array.is_some() =>
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
    let projection = fmt(value, ctx);
    let mut clauses = vec![format!("FROM {from}")];
    let inline_body = |clauses: &[String]| {
        clauses.iter().map(|part| part.trim_start()).collect::<Vec<_>>().join(" ")
    };
    let predicate_text = |e: &Expr, ctx: &mut crate::FormatCtx| {
        let comparison = e.as_binop().and_then(|(op, left, right)|
            reversed_comparison(op).map(|reverse| (op, reverse, left, right)));
        let Some((op, reverse, left, right)) = comparison else {
            return fmt(e, ctx);
        };
        if matches!(op, BinOpKind::Eq | BinOpKind::Ne) {
            let column = if right.is_null() && left.as_row_value().is_some() { Some(left) }
                else if left.is_null() && right.as_row_value().is_some() { Some(right) } else { None };
            if let Some(column) = column {
                let predicate = if op == BinOpKind::Eq { "IS NULL" } else { "IS NOT NULL" };
                return format!("{} {predicate}", fmt(column, ctx));
            }
        }
        // Keep physical table columns on the left unless the other operand
        // is a lower external threshold with its own acquisition identity.
        let has_external = |value: &Expr| {
            let mut found = false;
            value.walk(&mut |part| {
                found |= matches!(part.as_sym_var(), Some(SymVarOrigin::External { .. }));
            });
            found
        };
        let (mut op, reverse, a, z) =
            if left.as_row_value().is_none() && right.as_row_value().is_some() && !has_external(left) {
                (reverse, op, right, left)
            } else {
                (op, reverse, left, right)
            };
        let (a, z) = if matches!(op, BinOpKind::Ge | BinOpKind::Gt) && has_external(z) {
            op = reverse;
            (z, a)
        } else {
            (a, z)
        };
        let token = super::binop_display_token(op);
        format!("{} {token} {}", fmt(a, ctx), fmt(z, ctx))
    };
    for (i, p) in predicates.iter().enumerate() {
        clauses.push(format!(
            "{} {}",
            if i == 0 { "WHERE" } else { "  AND" },
            predicate_text(p, ctx)
        ))
    }
    let nullable_first = kind == math::RelationAggregateKind::FirstValue
        && value.as_row_value().is_some_and(|column| ctx.nullable_columns.contains(column))
        && default.is_some_and(|fallback| !fallback.is_null());
    let presence = nullable_first.then(|| {
        format!("EXISTS (SELECT 1 {})", inline_body(&clauses))
    });
    if !keys.is_empty() {
        clauses.push(format!(
            "ORDER BY {}",
            keys.iter()
                .map(|k| fmt(k, ctx))
                .collect::<Vec<_>>()
                .join(", ")
        ))
    }
    if scalar_array.is_some() && kind == math::RelationAggregateKind::Count {
        let prefix = if distinct { "DISTINCT " } else { "" };
        return Some(format!("COUNT({prefix}{projection} {})", clauses.join(" ")));
    }
    if kind == math::RelationAggregateKind::Exists {
        return Some(format!("EXISTS (SELECT {projection} {})", inline_body(&clauses)));
    }
    if kind == math::RelationAggregateKind::FirstValue && (default.is_some_and(Expr::is_null) || nullable_first) {
        let first = format!("FIRST({projection} {})", inline_body(&clauses));
        if let Some(presence) = presence {
            let fallback = fmt(default?, ctx);
            // CASE distinguishes a NULL on the selected row from absent rows.
            return Some(format!("(CASE WHEN {presence} THEN {first} ELSE {fallback} END)"));
        }
        return Some(first);
    }
    let (lead, default) = match kind {
        math::RelationAggregateKind::Sum => ("SUM(", "0".into()),
        math::RelationAggregateKind::FirstValue => {
            clauses.push("LIMIT 1".into());
            ("(", fmt(default?, ctx))
        }
        _ => return None,
    };
    if scalar_array.is_some() || record_array || nested || correlated {
        return Some(format!("COALESCE({lead}SELECT {projection} {}), {default})", clauses.join(" ")));
    }
    Some(format!(
        "COALESCE(\n  {lead}\n    SELECT {projection}\n    {}\n  ),\n  {default}\n)",
        clauses.join("\n    ")
    ))
}
