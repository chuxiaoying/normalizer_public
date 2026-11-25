//! Boundary observation of atomic SQL writes. Java owns evaluation of the
//! containing method and its control flow; the operation binds one completion.
use super::{
    fail, need, Engine, Env, Result, Ty, Value,
};
use math::{builder as b, domains::completion, Expr, Lit};
use normalizer_ir::local::{SqlOperation, ScalarOperand, SqlWrite};
use serde_json::json;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct SqlEffect {
    statement: SqlWrite,
    columns: BTreeMap<String, String>,
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
            let ty = match ty {
                Ty::Integer => "number",
                Ty::Text => "text",
                Ty::Bool => "bool",
                _ => return Err(fail("unsupported SQL column type")),
            };
            if let Some(previous) = columns.insert(column.clone(), ty.into()) {
                need(previous == ty, "incompatible bindings for one column")?;
            }
            args.push(value);
        }
        let guard = self.normal_path(path);
        if guard.as_bool() == Some(false) {
            return Ok(());
        }
        let response = b::sym_e(self.fresh_binding(), "integer");
        let failed = completion::is_throw(response.clone(), &write.exception);
        let failure_guard = math::normalize(b::and(guard.clone(), failed.clone()));
        self.realtime_effects.record_sql(
            guard,
            SqlEffect {
                statement: write.clone(),
                columns,
            },
            args,
            response.clone(),
        );

        self.realtime_throws.push(failure_guard);
        self.realtime_completion = self.realtime_completion.replace_continue_with_throw(
            failed,
            &write.exception,
            b::lit_null(),
        );
        // A single-row atomic INSERT with no conflict clause affects one row
        // on normal completion. UPDATE preserves the actual matched row count.
        let affected = match write.operation {
            SqlOperation::Insert => b::lit_int(1),
            SqlOperation::Update { .. } | SqlOperation::Delete { .. } => response,
        };
        env.insert(write.binding.clone(), Value::Term(Ty::Integer, affected));
        Ok(())
    }
}

impl SqlEffect {
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
            if let Some(previous) = table.insert(column.clone(), ty.clone()) {
                need(previous == *ty, "inconsistent SQL relation schema")?;
            }
        }
        let operands = args.iter().map(|operand| match operand.as_lit() {
            Some(Lit::Str(text)) => format!("'{}'", text.replace('\'', "''")),
            _ => crate::format_expr_with_ctx(operand, ctx),
        }).collect::<Vec<_>>();
        Ok(json!({
            "sql": render_statement(&self.statement, &operands)?,
            "returns": "number",
            "result": receipt
        }))
    }
}

fn render_statement(write: &SqlWrite, args: &[String]) -> Result<String> {
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
                let clause = predicates
                    .iter()
                    .zip(conditions)
                    .map(|((column, _), value)| format!("{table}.{column} = {value}"))
                    .collect::<Vec<_>>()
                    .join("\n  AND ");
                sql.push_str(&format!("\nWHERE {clause}"));
            }
            sql
        }
    })
}
