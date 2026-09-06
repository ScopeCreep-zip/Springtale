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
            output::emit(json_out, &body, |v| {
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
            })?;
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
