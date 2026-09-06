//! `springtale auth` — the API tokens the daemon has issued.
//!
//! `springtale login` mints one and writes it to the token file; this
//! family is how you see the rest and revoke one you no longer trust.

use anyhow::Result;
use serde_json::Value;

use crate::cli::AuthAction;
use crate::client::Client;
use crate::output;

/// Handle auth subcommands.
pub async fn run(action: AuthAction, json_out: bool) -> Result<()> {
    let client = Client::from_config()?;
    match action {
        AuthAction::Tokens => {
            let body: Value = client.get("/auth/tokens").await?;
            output::emit(json_out, &body, |v| {
                let rows = output::array(v, "tokens")
                    .iter()
                    .map(|t| {
                        vec![
                            output::cell(t, "id"),
                            output::cell(t, "name"),
                            output::cell(t, "created_at"),
                            output::cell(t, "last_used_at"),
                        ]
                    })
                    .collect();
                output::rows_table(&["ID", "NAME", "CREATED", "LAST USED"], rows)
            })?;
        }
        AuthAction::Revoke { id } => {
            let body: Value = client.delete(&format!("/auth/tokens/{id}")).await?;
            output::emit_status(json_out, &body, |_| format!("Revoked token {id}."))?;
        }
    }
    Ok(())
}
