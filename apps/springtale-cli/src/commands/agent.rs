//! `springtale agent` — per-agent settings, over the daemon.

use anyhow::Result;
use serde_json::{Value, json};

use crate::cli::AgentAction;
use crate::client::Client;
use crate::output;

/// Handle agent subcommands.
pub async fn run(action: AgentAction, json_out: bool) -> Result<()> {
    let client = Client::from_config()?;
    match action {
        AgentAction::SetAutonomy { name, level } => {
            // The daemon resolves the rule name or id to an autonomy
            // target — the CLI does not need the rule set to do it.
            let body: Value = client
                .put(
                    &format!("/agents/{name}/autonomy"),
                    &json!({ "level": level }),
                )
                .await?;
            output::emit(json_out, &body, |_| {
                format!("Agent '{name}' autonomy set to: {level}")
            })?;
        }
    }
    Ok(())
}
