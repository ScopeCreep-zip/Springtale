//! `springtale rule` — automation rules, over the daemon.
//!
//! Writes go to the running daemon, so a rule added here is scheduled
//! and its triggers attached immediately; no restart, and no second
//! writer against the SQLite file.

use anyhow::Result;
use serde_json::Value;

use springtale_core::rule::types::Rule;

use crate::cli::RuleAction;
use crate::client::Client;
use crate::output;

/// Handle rule subcommands.
pub async fn run(action: RuleAction, json_out: bool) -> Result<()> {
    let client = Client::from_config()?;
    match action {
        RuleAction::List => {
            let body: Value = client.get("/rules").await?;
            output::emit(json_out, &body, |v| {
                let rows = output::array(v, "rules")
                    .iter()
                    .map(|r| {
                        vec![
                            output::cell(r, "id"),
                            output::cell(r, "name"),
                            output::cell(r, "status"),
                            output::cell(r, "trigger"),
                        ]
                    })
                    .collect();
                output::rows_table(&["ID", "NAME", "STATUS", "TRIGGER"], rows)
            })?;
        }
        RuleAction::Add { file } => {
            let rule = load_rule(&file)?;
            let body: Value = client.post("/rules", &rule).await?;
            output::emit(json_out, &body, |v| {
                format!("Added rule: {} (id: {})", rule.name, output::cell(v, "id"))
            })?;
        }
        RuleAction::Update { id, file } => {
            let rule = load_rule(&file)?;
            let body: Value = client.put(&format!("/rules/{id}"), &rule).await?;
            output::emit(json_out, &body, |_| format!("Updated rule: {id}"))?;
        }
        RuleAction::Delete { id } => {
            let body: Value = client.delete(&format!("/rules/{id}")).await?;
            output::emit(json_out, &body, |_| format!("Deleted rule: {id}"))?;
        }
        RuleAction::Run { id } => {
            let body: Value = client
                .post(&format!("/rules/{id}/run"), &serde_json::json!({}))
                .await?;
            output::emit(json_out, &body, |v| {
                serde_json::to_string_pretty(v).unwrap_or_default()
            })?;
        }
        RuleAction::Toggle { id } => {
            // The route takes the target state, so read the current one
            // from the daemon rather than guessing.
            let listing: Value = client.get("/rules").await?;
            let current = output::array(&listing, "rules")
                .iter()
                .find(|r| output::cell(r, "id") == id)
                .ok_or_else(|| anyhow::anyhow!("rule not found: {id}"))?;
            let enabled = output::cell(current, "status") != "Enabled";
            let body: Value = client
                .post(
                    &format!("/rules/{id}/toggle"),
                    &serde_json::json!({ "enabled": enabled }),
                )
                .await?;
            output::emit(json_out, &body, |_| {
                format!(
                    "Rule {id} is now {}",
                    if enabled { "enabled" } else { "disabled" }
                )
            })?;
        }
    }
    Ok(())
}

/// Parse a rule definition from a JSON or TOML file.
fn load_rule(file: &std::path::Path) -> Result<Rule> {
    let contents = std::fs::read_to_string(file)
        .map_err(|e| anyhow::anyhow!("failed to read rule file at {}: {e}", file.display()))?;
    match file.extension().and_then(|ext| ext.to_str()) {
        Some("toml") => {
            toml::from_str(&contents).map_err(|e| anyhow::anyhow!("failed to parse rule TOML: {e}"))
        }
        Some("json") => serde_json::from_str(&contents)
            .map_err(|e| anyhow::anyhow!("failed to parse rule JSON: {e}")),
        _ => toml::from_str(&contents).or_else(|_| {
            serde_json::from_str(&contents).map_err(|e| {
                anyhow::anyhow!("failed to parse rule file (tried TOML and JSON): {e}")
            })
        }),
    }
}
