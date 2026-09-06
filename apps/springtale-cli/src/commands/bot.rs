//! `springtale bot` subcommands — pairing management from the daemon host.
//!
//! These commands run on the trusted device (the server), never via chat.
//! They open the encrypted database directly so they work without the
//! daemon running — critical for the `panic-unpair` IPV scenario.

use anyhow::{Context, Result};

use crate::cli::BotSettingsAction;
use crate::client::Client;
use crate::output;
use crate::store::PassphraseOpts;
use springtale_runtime::operations::pairing;

pub async fn pair_init(opts: &PassphraseOpts, json_out: bool) -> Result<()> {
    let store = crate::store::open_store(opts)?;
    let code = pairing::generate_pairing_code(&store)
        .await
        .context("failed to generate pairing code")?;

    let body = serde_json::json!({ "pairing_code": code, "single_use": true });
    output::emit(json_out, &body, |v| {
        format!(
            "Pairing code (give this to the user, do NOT send via chat):\n\n  {}\n\nThe user types this code into their chat with the bot.\nCode expires in 10 minutes. Single-use.",
            output::cell(v, "pairing_code")
        )
    })
}

pub async fn panic_unpair(opts: &PassphraseOpts, json_out: bool) -> Result<()> {
    let store = crate::store::open_store(opts)?;
    let removed = pairing::panic_unpair(&store)
        .await
        .context("failed to revoke paired users")?;

    let body = serde_json::json!({ "removed": removed });
    output::emit(json_out, &body, |_| {
        let tail = if removed > 0 {
            "All users must re-pair to regain access."
        } else {
            "No paired users were found."
        };
        format!("Removed {removed} pairing/paired entries.\n{tail}")
    })
}

/// `springtale bot status` — what the runtime is doing right now.
pub async fn status(json_out: bool) -> Result<()> {
    read(json_out, "/bot/status").await
}

/// `springtale bot formations` — the formations the bot is running.
pub async fn formations(json_out: bool) -> Result<()> {
    read(json_out, "/bot/formations").await
}

/// `springtale bot memory` — the session memory the bot is holding.
pub async fn memory(json_out: bool) -> Result<()> {
    read(json_out, "/bot/memory").await
}

/// GET one read-only bot view and print it.
async fn read(json_out: bool, path: &str) -> Result<()> {
    let client = Client::from_config()?;
    let body: serde_json::Value = client.get(path).await?;
    output::emit(json_out, &body, |v| {
        serde_json::to_string_pretty(v).unwrap_or_default()
    })
}

/// `springtale bot settings …` — plan 6.3. Goes through the daemon so the
/// change reaches the live runtime (a direct store write would only be
/// picked up on the next restart, which is the thing this replaced).
pub async fn settings(action: BotSettingsAction, json_out: bool) -> Result<()> {
    let client = Client::from_config()?;
    match action {
        BotSettingsAction::Get => {
            let body: serde_json::Value = client.get("/bot/settings").await?;
            output::emit(json_out, &body, |v| {
                serde_json::to_string_pretty(v).unwrap_or_default()
            })?;
        }
        BotSettingsAction::Set {
            name,
            tone,
            prefix,
            context_window,
            allow,
        } => {
            // Read-modify-write: the endpoint takes the whole document, so
            // unspecified flags have to carry the stored value forward.
            let mut settings: serde_json::Value = client.get("/bot/settings").await?;
            if let Some(name) = name {
                settings["persona"]["name"] = serde_json::json!(name);
            }
            if let Some(tone) = tone {
                settings["persona"]["tone"] = serde_json::json!(tone);
            }
            if let Some(prefix) = prefix {
                settings["persona"]["prefix"] = serde_json::json!(prefix);
            }
            if let Some(window) = context_window {
                settings["context_window"] = serde_json::json!(window);
            }
            if !allow.is_empty() {
                let entries: Vec<String> = allow.into_iter().filter(|a| !a.is_empty()).collect();
                settings["tool_policy"]["allow"] = serde_json::json!(entries);
            }

            let body: serde_json::Value = client
                .put("/bot/settings", &settings)
                .await
                .context("failed to save bot settings")?;
            output::emit(json_out, &body, |_| "bot settings saved".to_owned())?;
        }
    }
    Ok(())
}
