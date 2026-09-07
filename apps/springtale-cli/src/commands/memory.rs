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
            output::emit(json_out, &body, audit_table)?;
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

/// The `memory audit` view — the note plus one row per live session.
fn audit_table(v: &Value) -> String {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::{json_value, key_set};

    fn audit() -> Value {
        json!({
            "total_memory_note": "3 sessions holding 42 entries",
            "sessions": [{
                "user_id": "u-1",
                "channel_id": "c-1",
                "created_at": "2026-09-04T10:00:00Z",
            }]
        })
    }

    #[test]
    fn test_memory_audit_json_shape_has_the_note_and_the_sessions() {
        let out = json_value(&audit());
        assert_eq!(key_set(&out), ["sessions", "total_memory_note"]);
        assert!(out["total_memory_note"].is_string());
        assert!(out["sessions"].is_array());
        let session = &out["sessions"][0];
        assert!(session["user_id"].is_string());
        assert!(session["channel_id"].is_string());
        assert!(session["created_at"].is_string());
    }

    #[test]
    fn test_audit_table_reads_every_field_the_json_shape_promises() {
        let table = audit_table(&audit());
        for want in ["3 sessions holding 42 entries", "USER", "u-1", "c-1"] {
            assert!(table.contains(want), "table lost {want}:\n{table}");
        }
    }

    #[test]
    fn test_audit_table_says_so_when_no_session_is_live() {
        let table = audit_table(&json!({ "total_memory_note": "none", "sessions": [] }));
        assert!(table.ends_with("No active sessions."));
    }

    #[test]
    fn test_memory_compact_json_shape_reports_what_it_trimmed() {
        let out = json_value(&json!({ "sessions_compacted": 2, "entries_removed": 11 }));
        assert_eq!(key_set(&out), ["entries_removed", "sessions_compacted"]);
        assert!(out["sessions_compacted"].is_number());
        assert!(out["entries_removed"].is_number());
    }
}
