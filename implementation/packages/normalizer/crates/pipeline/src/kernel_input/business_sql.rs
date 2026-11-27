//! Boundary observation of atomic SQL writes. Java owns evaluation of the
//! containing method and its control flow; the operation binds one completion.
use super::{
    fail, need, Engine, Env, Result, Ty, Value,
};
use math::{builder as b, domains::completion, Expr, Lit};
use normalizer_ir::local::{SqlOperation, ScalarOperand, SqlWrite};
use serde_json::json;
use std::collections::{btree_map::Entry, BTreeMap};

pub(super) enum ColumnEvidence {
    Declaration,
    Operand,
}

/// A table has one business type per physical column. A write operand
/// can confirm the base type, but cannot override declared nullability.
pub(super) fn register_column(
    columns: &mut BTreeMap<String, String>,
    table: &str,
    name: &str,
    ty: &str,
    evidence: ColumnEvidence,
) -> Result<()> {
    match columns.entry(name.into()) {
        Entry::Vacant(column) => { column.insert(ty.into()); }
        Entry::Occupied(column) => {
            let prior = column.get().as_str();
            let expected = match evidence {
                ColumnEvidence::Declaration => prior,
                ColumnEvidence::Operand => prior.strip_suffix('?').unwrap_or(prior),
            };
            if expected != ty {
                return Err(fail(format!("conflicting physical column type: {table}.{name}")));
            }
        }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct SqlEffect {
    statement: SqlWrite,
    columns: BTreeMap<String, String>,
}

/// One primitive database-column type mapping for local relations and SQL writes.
pub(super) fn column_type(ty: &Ty, nullable_text: bool) -> Option<&'static str> {
    match ty {
        Ty::Integer => Some("number"),
        Ty::Text if nullable_text => Some("text?"),
        Ty::Text => Some("text"),
        _ => None,
    }
}

impl Engine<'_> {
    pub(super) fn execute_sql_write(
        &mut self,
        write: &SqlWrite,
        env: &mut Env,
        path: Expr,
    ) -> Result<()> {
        need(
            self.project.exceptions.contains_key(&write.exception),
            "undeclared SQL failure type",
        )?;
        let mut args = Vec::new();
        let mut columns = BTreeMap::new();
        for (column, operand) in write.operands() {
            let (ty, value) = match operand {
                ScalarOperand::Local(name) => {
                    let value = env
                        .get(name)
                        .ok_or_else(|| fail(format!("unbound SQL operand {name}")))?;
                    (value.ty(), value.scalar()?)
                }
                ScalarOperand::Integer(n) => (Ty::Integer, b::lit_integer(n.clone())),
                ScalarOperand::Text(s) => (Ty::Text, b::lit_str(s)),
            };
            let ty = column_type(&ty, false).ok_or_else(|| fail("unsupported SQL column type"))?;
            register_column(&mut columns, &write.table, column, ty, ColumnEvidence::Operand)?;
            args.push(value);
        }
        let guard = self.normal_path(path);
        let receipt = super::realtime_state::EffectReceipt::sql_count(self.fresh_binding());
        let response = receipt.response().clone();
        let failed = completion::is_throw(response.clone(), &write.exception);
        let failure_guard = math::normalize(b::and(guard.clone(), failed.clone()));
        self.realtime_effects.record_sql(
            guard,
            SqlEffect {
                statement: write.clone(),
                columns,
            },
            args,
            receipt,
        );

        self.realtime_throws.push(failure_guard.clone());
        self.realtime_completion = self.realtime_completion.replace_continue_with_throw(
            failure_guard,
            &write.exception,
            None,
        );
        // The admitted single-row INSERT directive carries the seed's
        // contract of exactly one committed row on success. Preprocessing
        // must establish that contract, not merely recognize the SQL token.
        // UPDATE and DELETE expose their actual affected-row receipts.
        let affected = match write.operation {
            SqlOperation::Insert => b::lit_int(1),
            SqlOperation::Update { .. } | SqlOperation::Delete { .. } => response,
        };
        env.insert(write.binding.clone(), Value::Term(Ty::Integer, affected));
        Ok(())
    }
}

impl SqlEffect {
    pub(super) fn table(&self) -> &str { &self.statement.table }

    pub(super) fn exception(&self) -> &str {
        &self.statement.exception
    }

    pub(super) fn project(
        &self,
        args: &[Expr],
        receipt: &str,
        tables: &mut BTreeMap<String, BTreeMap<String, String>>,
        ctx: &mut crate::FormatCtx,
    ) -> Result<serde_json::Value> {
        let table = tables.entry(self.statement.table.clone()).or_default();
        for (column, ty) in &self.columns {
            register_column(table, &self.statement.table, column, ty, ColumnEvidence::Operand)?;
        }
        let operands = args.iter().map(|operand| match operand.as_lit() {
            Some(Lit::Str(text)) => format!("'{}'", text.replace('\'', "''")),
            _ => crate::format_expr_with_ctx(operand, ctx),
        }).collect::<Vec<_>>();
        let predicate_order = match &self.statement.operation {
            SqlOperation::Insert => vec![],
            SqlOperation::Update { predicates } | SqlOperation::Delete { predicates } => {
                let compare = ctx.bound_order();
                let offset = self.statement.assignments.len();
                let mut keys = predicates.iter().enumerate().map(|(index, (column, _))| {
                    let kind = match table[column].as_str() {
                        "number" => "number",
                        "text" => "text",
                        "text?" => "text?",
                        _ => unreachable!("SQL assignment checked scalar column type"),
                    };
                    let slot = b::sql_column(&self.statement.table, math::domains::relation::TableSource {
                        name: self.statement.table.clone(), snapshot: None,
                    }, 0, column, kind);
                    let value = args[offset + index].clone();
                    let expression = if math::canonical_order_ordered_comparison(
                        math::BinOpKind::Eq, &slot, &value, &compare,
                    ).is_some() { b::eq(value, slot) } else { b::eq(slot, value) };
                    (math::canonical_order_key(&expression, &compare), index)
                }).collect::<Vec<_>>();
                keys.sort_by(|a, z| a.0.cmp(&z.0));
                keys.into_iter().map(|(_, index)| index).collect()
            }
        };
        Ok(json!({
            "sql": render_statement(&self.statement, &operands, &predicate_order)?,
            "returns": "number",
            "result": receipt
        }))
    }
}

fn render_statement(write: &SqlWrite, args: &[String], predicate_order: &[usize]) -> Result<String> {
    need(
        write.operands().count() == args.len(),
        "SQL operand count mismatch",
    )?;
    let columns: Vec<_> = write
        .assignments
        .iter()
        .map(|(column, _)| column.as_str())
        .collect();
    let (values, conditions) = args.split_at(columns.len());
    let table = &write.table;
    Ok(match &write.operation {
        SqlOperation::Insert => format!(
            "INSERT INTO {table} ({})\nVALUES ({})",
            columns.join(", "),
            values.join(", ")
        ),
        SqlOperation::Update { predicates } | SqlOperation::Delete { predicates } => {
            let assignments = columns
                .iter()
                .zip(values)
                .map(|(column, value)| format!("{column} = {value}"))
                .collect::<Vec<_>>()
                .join(", ");
            let mut sql = if matches!(write.operation, SqlOperation::Delete { .. }) {
                format!("DELETE FROM {table}")
            } else {
                format!("UPDATE {table}\nSET {assignments}")
            };
            if !conditions.is_empty() {
                let clause = predicate_order
                    .iter()
                    .map(|&index| {
                        let (column, _) = &predicates[index];
                        let value = &conditions[index];
                        format!("{table}.{column} = {value}")
                    })
                    .collect::<Vec<_>>()
                    .join("\n  AND ");
                sql.push_str(&format!("\nWHERE {clause}"));
            }
            sql
        }
    })
}
