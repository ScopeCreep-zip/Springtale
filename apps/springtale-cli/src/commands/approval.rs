//! `springtale approval` — the blocking gate for dangerous capabilities.

use anyhow::Result;
use serde_json::{Value, json};

use crate::cli::ApprovalAction;
use crate::client::Client;
use crate::output;

/// Handle approval subcommands.
pub async fn run(action: ApprovalAction, json_out: bool) -> Result<()> {
    let client = Client::from_config()?;
    match action {
        ApprovalAction::List => {
            let body: Value = client.get("/approvals").await?;
            output::emit(json_out, &body, pending_table)?;
        }
        ApprovalAction::Approve { id, reason } => {
            resolve(&client, json_out, &id, "approve", reason).await?;
        }
        ApprovalAction::Deny { id, reason } => {
            resolve(&client, json_out, &id, "deny", reason).await?;
        }
    }
    Ok(())
}

async fn resolve(
    client: &Client,
    json_out: bool,
    id: &str,
    decision: &str,
    reason: Option<String>,
) -> Result<()> {
    let body: Value = client
        .post(
            &format!("/approvals/{id}"),
            &json!({ "decision": decision, "reason": reason }),
        )
        .await?;
    output::emit(json_out, &body, |_| format!("{id}: {decision}d"))
}

/// The `approval list` table — one row per pending request.
fn pending_table(v: &Value) -> String {
    let rows = output::array(v, "pending")
        .iter()
        .map(|p| {
            vec![
                output::cell(p, "id"),
                output::cell(p, "capability"),
                output::cell(p, "requested_at"),
            ]
        })
        .collect();
    output::rows_table(&["ID", "CAPABILITY", "REQUESTED"], rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::{json_value, key_set};

    fn queue() -> Value {
        json!({
            "pending": [{
                "id": "ap-1",
                "capability": "ShellExec",
                "requested_at": "2026-09-04T10:00:00Z",
            }]
        })
    }

    #[test]
    fn test_approval_list_json_shape_is_a_pending_envelope() {
        let out = json_value(&queue());
        assert_eq!(key_set(&out), ["pending"]);
        assert!(out["pending"].is_array());
        let item = &out["pending"][0];
        assert!(item["id"].is_string());
        assert!(item["capability"].is_string());
        assert!(item["requested_at"].is_string());
    }

    #[test]
    fn test_pending_table_reads_every_field_the_json_shape_promises() {
        let table = pending_table(&queue());
        for want in ["ID", "ap-1", "ShellExec", "2026-09-04T10:00:00Z"] {
            assert!(table.contains(want), "table lost {want}:\n{table}");
        }
    }

    #[test]
    fn test_pending_table_is_empty_when_nothing_is_queued() {
        assert_eq!(pending_table(&json!({ "pending": [] })), "");
    }
}
