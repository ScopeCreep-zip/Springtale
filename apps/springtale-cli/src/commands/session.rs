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
            output::emit(json_out, &body, |v| {
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
            })?;
        }
    }
    Ok(())
}
