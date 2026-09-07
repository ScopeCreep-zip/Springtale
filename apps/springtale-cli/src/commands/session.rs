//! `springtale session` — chat sessions the daemon is holding.

use anyhow::Result;
use serde_json::Value;

use crate::cli::SessionAction;
use crate::client::Client;
use crate::output;

/// Handle session subcommands.
pub async fn run(action: SessionAction, json_out: bool) -> Result<()> {
    let client = Client::from_config()?;
    match action {
        SessionAction::List => {
            let body: Value = client.get("/sessions").await?;
            output::emit(json_out, &body, sessions_table)?;
        }
    }
    Ok(())
}

/// The `session list` table — one row per chat session.
fn sessions_table(v: &Value) -> String {
    let rows = output::array(v, "sessions")
        .iter()
        .map(|s| {
            vec![
                output::cell(s, "user_id"),
                output::cell(s, "channel_id"),
                output::cell(s, "created_at"),
            ]
        })
        .collect();
    output::rows_table(&["USER", "CHANNEL", "CREATED"], rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::{json_value, key_set};
    use serde_json::json;

    fn sessions() -> Value {
        json!({
            "sessions": [{
                "user_id": "u-1",
                "channel_id": "c-1",
                "created_at": "2026-09-04T10:00:00Z",
            }]
        })
    }

    #[test]
    fn test_session_list_json_shape_is_a_sessions_envelope() {
        let out = json_value(&sessions());
        assert_eq!(key_set(&out), ["sessions"]);
        assert!(out["sessions"].is_array());
        let session = &out["sessions"][0];
        assert!(session["user_id"].is_string());
        assert!(session["channel_id"].is_string());
        assert!(session["created_at"].is_string());
    }

    #[test]
    fn test_sessions_table_reads_every_field_the_json_shape_promises() {
        let table = sessions_table(&sessions());
        for want in ["USER", "u-1", "c-1", "2026-09-04T10:00:00Z"] {
            assert!(table.contains(want), "table lost {want}:\n{table}");
        }
    }

    #[test]
    fn test_sessions_table_is_empty_when_no_session_is_held() {
        assert_eq!(sessions_table(&json!({ "sessions": [] })), "");
    }
}
