//! `springtale memory` — bot memory inspection and maintenance.

use anyhow::Result;
use serde_json::{Value, json};

use crate::cli::MemoryAction;
use crate::client::Client;
use crate::output;

/// Handle memory subcommands.
pub async fn run(action: MemoryAction, json_out: bool) -> Result<()> {
    let client = Client::from_config()?;
    match action {
        MemoryAction::Audit => {
            let body: Value = client.post("/memory/audit", &json!({})).await?;
            output::emit(json_out, &body, |v| {
                let mut out = output::cell(v, "total_memory_note");
                let rows: Vec<Vec<String>> = output::array(v, "sessions")
                    .iter()
                    .map(|s| {
                        vec![
                            output::cell(s, "user_id"),
                            output::cell(s, "channel_id"),
                            output::cell(s, "created_at"),
                        ]
                    })
                    .collect();
                let table = output::rows_table(&["USER", "CHANNEL", "CREATED"], rows);
                if table.is_empty() {
                    out.push_str("\nNo active sessions.");
                } else {
                    out.push('\n');
                    out.push_str(&table);
                }
                out
            })?;
        }
        MemoryAction::Compact { max_entries } => {
            let body: Value = client
                .post("/memory/compact", &json!({ "max_entries": max_entries }))
                .await?;
            output::emit(json_out, &body, |_| {
                format!("Compacted to at most {max_entries} entries per session.")
            })?;
        }
    }
    Ok(())
}
