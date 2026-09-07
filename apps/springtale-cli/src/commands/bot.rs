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

pub async fn pair_init(json_out: bool) -> Result<()> {
    let client = Client::from_config()?;
    let body: serde_json::Value = client
        .post("/bot/pair-init", &serde_json::json!({}))
        .await?;
    output::emit(json_out, &body, |v| {
        format!(
            "Pairing code (give this to the user, do NOT send via chat):\n\n  {}\n\nThe user types this code into their chat with the bot.\nCode expires in 10 minutes. Single-use.",
            output::cell(v, "pairing_code")
        )
    })
}

/// `springtale bot panic-unpair` — revoke every pairing, offline.
///
/// This one does NOT go through the daemon, on purpose. It is reached
/// when the phone or the account on the other end of a pairing is in the
/// wrong hands, from whatever terminal the user has recovered, and it
/// has to work when springtaled is dead, wedged, or the very thing that
/// has been taken. `springtale panic` is offline for the same reason.
/// The write is a delete of every `paired_user:` / `pairing_code:` /
/// `pairing_rate:` row, so a daemon that is running simply stops finding
/// them; there is nothing for it to have been told.
pub async fn panic_unpair(opts: &PassphraseOpts, json_out: bool) -> Result<()> {
    let store = crate::store::open_store(opts)?;
    let removed = pairing::panic_unpair(&store)
        .await
        .context("failed to revoke paired users")?;

    let body = unpair_body(removed);
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

/// The `bot pair-init` body as the daemon returns it — the code the
/// operator reads out, plus the single-use contract it comes with. The
/// route builds this now; the shape is kept here so the rendering test
/// asserts against the real one.
#[cfg(test)]
fn pair_init_body(code: &str) -> serde_json::Value {
    serde_json::json!({ "pairing_code": code, "single_use": true })
}

/// The `bot panic-unpair` body — how many pairing rows were revoked.
fn unpair_body(removed: u32) -> serde_json::Value {
    serde_json::json!({ "removed": removed })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::{json_value, key_set};

    #[test]
    fn test_bot_pair_init_json_shape_names_the_code_and_single_use() {
        let out = json_value(&pair_init_body("TRUE-BADGER-9142"));
        assert_eq!(key_set(&out), ["pairing_code", "single_use"]);
        assert!(out["pairing_code"].is_string());
        assert_eq!(out["pairing_code"], "TRUE-BADGER-9142");
        assert!(out["single_use"].is_boolean());
        assert_eq!(out["single_use"], true);
    }

    #[test]
    fn test_bot_panic_unpair_json_shape_is_a_removed_count() {
        let out = json_value(&unpair_body(3));
        assert_eq!(key_set(&out), ["removed"]);
        assert!(out["removed"].is_number());
        assert_eq!(out["removed"], 3);
    }

    #[test]
    fn test_bot_panic_unpair_json_reports_zero_rather_than_omitting_it() {
        let out = json_value(&unpair_body(0));
        assert_eq!(out["removed"], 0);
    }
}
