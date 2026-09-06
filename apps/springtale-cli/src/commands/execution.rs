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
            output::emit(json_out, &body, |v| {
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
            })?;
        }
        ExecutionAction::Steps { id } => {
            let body: Value = client.get(&format!("/executions/{id}/steps")).await?;
            output::emit(json_out, &body, |v| {
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
            })?;
        }
        ExecutionAction::Vacuum { keep_days } => {
            let body: Value = client
                .post("/executions/vacuum", &json!({ "keep_days": keep_days }))
                .await?;
            output::emit_status(json_out, &body, |v| {
                format!("Vacuumed executions: {}", output::cell(v, "deleted"))
            })?;
        }
    }
    Ok(())
}
