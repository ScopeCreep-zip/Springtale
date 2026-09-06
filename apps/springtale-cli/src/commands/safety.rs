//! `springtale safety` — app-level safety config, over the daemon.

use anyhow::Result;
use serde_json::{Value, json};

use crate::cli::SafetyAction;
use crate::client::Client;
use crate::output;

/// Handle safety subcommands.
pub async fn run(action: SafetyAction, json_out: bool) -> Result<()> {
    let client = Client::from_config()?;
    match action {
        SafetyAction::Get => {
            let body: Value = client.get("/safety").await?;
            output::emit(json_out, &body, |v| {
                serde_json::to_string_pretty(v).unwrap_or_default()
            })?;
        }
        SafetyAction::Disguise { active } => {
            let body: Value = client
                .post("/safety/disguise/active", &json!({ "active": active }))
                .await?;
            output::emit(json_out, &body, |_| {
                format!("disguise {}", if active { "on" } else { "off" })
            })?;
        }
        SafetyAction::DisguiseProfile { app_name, icon_id } => {
            let body: Value = client
                .post(
                    "/safety/disguise/profile",
                    &json!({ "app_name": app_name, "icon_id": icon_id }),
                )
                .await?;
            output::emit_status(json_out, &body, |_| {
                format!("disguise profile: {app_name} ({icon_id})")
            })?;
        }
        SafetyAction::PanicTaps { count } => {
            let body: Value = client
                .post("/safety/panic_tap_count", &json!({ "count": count }))
                .await?;
            output::emit(json_out, &body, |v| {
                format!("panic tap count: {}", output::cell(v, "panic_tap_count"))
            })?;
        }
    }
    Ok(())
}
