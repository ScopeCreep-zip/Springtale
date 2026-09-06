//! `springtale drift` — how far a deployed recipe or rule has drifted
//! from what it was applied as.

use anyhow::Result;
use serde_json::Value;

use crate::cli::DriftAction;
use crate::client::Client;
use crate::output;

/// Handle drift subcommands.
pub async fn run(action: DriftAction, json_out: bool) -> Result<()> {
    let client = Client::from_config()?;
    let body: Value = match &action {
        DriftAction::Recipe { id } => client.get(&format!("/drift/recipe/{id}")).await?,
        DriftAction::Rule { id } => client.get(&format!("/drift/rule/{id}")).await?,
    };
    output::emit(json_out, &body, |v| {
        serde_json::to_string_pretty(v).unwrap_or_default()
    })
}
