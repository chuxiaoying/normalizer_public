//! Business SELECT output for native relational aggregates.

use math::{
    domains::relation::{self as r, RelationExpr},
    BinOpKind, Expr, SymVarOrigin,
};

fn comparison(expr: &Expr) -> Option<(BinOpKind, BinOpKind, &Expr, &Expr)> {
    let (op, left, right) = expr.as_binop()?;
    Some((op, op.swapped_comparison()?, left, right))
}

fn symbol_origins(expr: &Expr) -> (bool, bool) {
    let (mut parameter, mut external) = (false, false);
    expr.walk(&mut |part| match part.as_sym_var() {
        Some(SymVarOrigin::Parameter { .. }) => parameter = true,
        Some(SymVarOrigin::External { .. }) => external = true,
        _ => {}
    });
    (parameter, external)
}

fn relation_name<'a>(physical: &'a str, snapshot: Option<u64>, ctx: &'a crate::FormatCtx) -> Option<&'a str> {
    match snapshot { Some(id) => ctx.acquisition_name(id), None => Some(physical) }
}

fn table_reference(physical: &str, snapshot: Option<u64>, ctx: &crate::FormatCtx) -> Option<String> {
    let relation = relation_name(physical, snapshot, ctx)?;
    Some(if snapshot.is_some() { format!("{relation} AS {physical}") } else { physical.into() })
}

pub(super) fn render(expr: &Expr, ctx: &mut crate::FormatCtx) -> Option<String> {
    let (kind, mut relation, mut value, default) = expr.as_relation_aggregate()?;
    let mut nth = None;
    if kind == math::RelationAggregateKind::NthValue {
        let index = crate::format_expr_with_ctx(value?, ctx);
        match relation {
            RelationExpr::RecordProjection { source, field } =>
                return Some(format!("(SELECT r.{field} FROM {source} AS r)[{index}]")),
            RelationExpr::Project { value: projection, input, alias } if alias == "value" => {
                if r::order_components(input).is_none() { return None; }
                relation = input;
                value = Some(projection);
                nth = Some(index);
            }
            _ => return None,
        }
    }
    if kind == math::RelationAggregateKind::Count && value.is_none() {
        if let RelationExpr::ProjectedUnion(branches) = relation {
            let distinct = if branches.iter().len() == 1 { "DISTINCT " } else { "" };
            let selection = branches.iter().map(|(table, column)| {
                let from = table_reference(&table.name, table.snapshot, ctx)?;
                Some(format!("  SELECT {distinct}{column} FROM {from}"))
            }).collect::<Option<Vec<_>>>()?.join("\n  UNION\n");
            return Some(format!("SELECT COUNT(*) FROM (\n{selection}\n) AS accounts"));
        }
    }
    let one = math::builder::lit_int(1);
    let value = if kind == math::RelationAggregateKind::Exists {
        &one
    } else {
        value?
    };
    if kind == math::RelationAggregateKind::Sum {
        if value == &one {
            if let RelationExpr::Grouped(group) = relation {
                let source = group.table();
                let table = &source.name;
                let from = table_reference(table, source.snapshot, ctx)?;
                let keys = group.columns().map(|column| format!("{table}.{column}")).collect::<Vec<_>>().join(", ");
                return Some(format!("COALESCE(\n  SUM(\n    SELECT 1\n    FROM (\n      SELECT {keys}\n      FROM {from}\n      GROUP BY {keys}\n    ) AS groups\n  ),\n  0\n)"));
            }
        }
        if let RelationExpr::Join { left, right, on, filters } = relation {
            let ((left, left_alias), (right, right_alias)) = (left, right);
            let left = relation_name(&left.name, left.snapshot, ctx)?.to_owned();
            let right = relation_name(&right.name, right.snapshot, ctx)?.to_owned();
            let rank = |expr: &Expr| {
                let mut left = false;
                let mut right = false;
                expr.walk(&mut |node| {
                    if let Some((alias, _)) = node.as_row_value().and_then(|p| p.split_once('.')) {
                        left |= alias == left_alias;
                        right |= alias == right_alias;
                    }
                });
                if left { 0 } else if right { 1 } else { 2 }
            };
            let fmt_predicate = |expr: &Expr, ctx: &mut crate::FormatCtx| {
                let Some((op, _, left, right)) = comparison(expr) else {
                    return crate::format_expr_with_ctx(expr, ctx);
                };
                let token = super::binop_display_token(op);
                format!("{} {token} {}", crate::format_expr_with_ctx(left, ctx), crate::format_expr_with_ctx(right, ctx))
            };
            let on = fmt_predicate(on, ctx);
            let mut filters = filters.iter().collect::<Vec<_>>();
            filters.sort_by_key(|f| (rank(f), math::canonical_order_key(f, &math::DefaultSortContext)));
            let mut out = format!("COALESCE(\n  SUM(\n    SELECT 1\n    FROM {left} AS {left_alias}\n    JOIN {right} AS {right_alias} ON {on}");
            for (i, f) in filters.into_iter().enumerate() {
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
        predicate.for_each_conjunct(|term| predicates.push(term));
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
        RelationExpr::Base { name, alias, snapshot: None } if record_array || nested => format!("{name} AS {alias}"),
        RelationExpr::Base { name, .. } if scalar_array.is_some() =>
            super::array_name(name, ctx).to_owned(),
        RelationExpr::Base { name, snapshot, .. } => table_reference(name, *snapshot, ctx)?,
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
        let (parameter, external) = symbol_origins(e);
        let mut rank = if parameter { 0 } else if external { 1 } else { 2 };
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
        let Some((op, reverse, left, right)) = comparison(e) else {
            return fmt(e, ctx);
        };
        if matches!(op, BinOpKind::Eq | BinOpKind::Ne) {
            if right.is_null() && left.as_row_value().is_some() {
                let predicate = if op == BinOpKind::Eq { "IS NULL" } else { "IS NOT NULL" };
                return format!("{} {predicate}", fmt(left, ctx));
            }
        }
        // Keep physical table columns on the left unless the other operand
        // is a lower external threshold with its own acquisition identity.
        let row_first = left.as_row_value().is_none() && right.as_row_value().is_some()
            && !symbol_origins(left).1;
        // If row_first swaps, its new RHS has already been proven nonexternal.
        let (op, a, z) = if row_first
            || (matches!(op, BinOpKind::Ge | BinOpKind::Gt) && symbol_origins(right).1)
        {
            (reverse, right, left)
        } else {
            (op, left, right)
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
    if let Some(index) = nth {
        return Some(format!("(SELECT {projection} {})[{index}]", inline_body(&clauses)));
    }
    if scalar_array.is_some() && kind == math::RelationAggregateKind::Count {
        let prefix = if distinct { "DISTINCT " } else { "" };
        return Some(format!("COUNT({prefix}{projection} {})", clauses.join(" ")));
    }
    if kind == math::RelationAggregateKind::Count
        && matches!(input, RelationExpr::Base { .. }) && !record_array && !nested && scalar_array.is_none()
    {
        let prefix = if distinct { "DISTINCT " } else { "" };
        return Some(format!("SELECT COUNT({prefix}{projection}) {}", inline_body(&clauses)));
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
        return Some(format!("COALESCE({lead}SELECT {projection} {}), {default})", inline_body(&clauses)));
    }
    Some(format!(
        "COALESCE(\n  {lead}\n    SELECT {projection}\n    {}\n  ),\n  {default}\n)",
        clauses.join("\n    ")
    ))
}
