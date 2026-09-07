//! `springtale execution` — the execution log the dashboard's run list
//! reads, and the vacuum that trims it.

use anyhow::Result;
use serde_json::{Value, json};

use crate::cli::ExecutionAction;
use crate::client::Client;
use crate::output;

/// The execution log collection. Query filters are appended to it.
const EXECUTIONS: &str = "/executions";

/// Handle execution subcommands.
pub async fn run(action: ExecutionAction, json_out: bool) -> Result<()> {
    let client = Client::from_config()?;
    match action {
        ExecutionAction::List { rule, limit } => {
            let mut query = format!("?limit={limit}");
            if let Some(rule) = &rule {
                query.push_str(&format!("&rule_id={rule}"));
            }
            let body: Value = client.get(&format!("{EXECUTIONS}{query}")).await?;
            output::emit(json_out, &body, executions_table)?;
        }
        ExecutionAction::Steps { id } => {
            let body: Value = client.get(&format!("/executions/{id}/steps")).await?;
            output::emit(json_out, &body, steps_table)?;
        }
        ExecutionAction::Vacuum { keep_days } => {
            let body: Value = client
                .post("/executions/vacuum", &json!({ "keep_days": keep_days }))
                .await?;
            output::emit_status(json_out, &body, vacuum_line)?;
        }
    }
    Ok(())
}

/// The `execution list` table — the route answers a bare array.
fn executions_table(v: &Value) -> String {
    let empty = Vec::new();
    let rows = v
        .as_array()
        .unwrap_or(&empty)
        .iter()
        .map(|e| {
            vec![
                output::cell(e, "id"),
                output::cell(e, "rule_id"),
                output::cell(e, "status"),
                output::cell(e, "started_at"),
            ]
        })
        .collect();
    output::rows_table(&["ID", "RULE", "STATUS", "STARTED"], rows)
}

/// The `execution steps` table — one row per step of a run.
fn steps_table(v: &Value) -> String {
    let empty = Vec::new();
    let rows = v
        .as_array()
        .unwrap_or(&empty)
        .iter()
        .map(|s| {
            vec![
                output::cell(s, "step_index"),
                output::cell(s, "action"),
                output::cell(s, "status"),
                output::cell(s, "duration_ms"),
            ]
        })
        .collect();
    output::rows_table(&["#", "ACTION", "STATUS", "MS"], rows)
}

/// The `execution vacuum` notice.
fn vacuum_line(v: &Value) -> String {
    format!("Vacuumed executions: {}", output::cell(v, "deleted"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::{json_value, key_set};

    fn runs() -> Value {
        json!([{
            "id": "x-1",
            "rule_id": "r-1",
            "status": "success",
            "started_at": "2026-09-04T10:00:00Z",
        }])
    }

    fn steps() -> Value {
        json!([{
            "step_index": 0,
            "action": "telegram.send_message",
            "status": "success",
            "duration_ms": 42,
        }])
    }

    #[test]
    fn test_execution_list_json_shape_is_a_bare_array_of_runs() {
        let out = json_value(&runs());
        assert!(out.is_array(), "the run list is not wrapped in an envelope");
        let run = &out[0];
        assert!(run["id"].is_string());
        assert!(run["rule_id"].is_string());
        assert!(run["status"].is_string());
        assert!(run["started_at"].is_string());
    }

    #[test]
    fn test_execution_steps_json_shape_is_a_bare_array_of_steps() {
        let out = json_value(&steps());
        assert!(out.is_array());
        let step = &out[0];
        assert!(step["step_index"].is_number());
        assert!(step["action"].is_string());
        assert!(step["status"].is_string());
        assert!(step["duration_ms"].is_number());
    }

    #[test]
    fn test_executions_table_reads_every_field_the_json_shape_promises() {
        let table = executions_table(&runs());
        for want in ["ID", "x-1", "r-1", "success", "2026-09-04T10:00:00Z"] {
            assert!(table.contains(want), "table lost {want}:\n{table}");
        }
    }

    #[test]
    fn test_steps_table_reads_every_field_the_json_shape_promises() {
        let table = steps_table(&steps());
        for want in ["ACTION", "telegram.send_message", "success", "42"] {
            assert!(table.contains(want), "table lost {want}:\n{table}");
        }
    }

    #[test]
    fn test_execution_tables_are_empty_for_an_empty_array() {
        assert_eq!(executions_table(&json!([])), "");
        assert_eq!(steps_table(&json!([])), "");
    }

    #[test]
    fn test_execution_vacuum_json_shape_reports_the_deleted_count() {
        let body = json!({ "deleted": 17 });
        let out = json_value(&body);
        assert_eq!(key_set(&out), ["deleted"]);
        assert!(out["deleted"].is_number());
        assert_eq!(vacuum_line(&body), "Vacuumed executions: 17");
    }
}
