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
            output::emit(json_out, &body, panic_taps_line)?;
        }
    }
    Ok(())
}

/// The `safety panic-taps` acknowledgement line.
fn panic_taps_line(v: &Value) -> String {
    format!("panic tap count: {}", output::cell(v, "panic_tap_count"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::{json_value, key_set};

    #[test]
    fn test_safety_get_json_is_the_daemon_config_document_untouched() {
        let config = json!({
            "disguise": { "active": false, "app_name": "Notes", "icon_id": "notes" },
            "panic_tap_count": 5,
            "auto_lock_secs": 300,
        });
        assert_eq!(json_value(&config), config);
    }

    #[test]
    fn test_safety_disguise_json_shape_reports_the_active_flag() {
        let out = json_value(&json!({ "active": true }));
        assert_eq!(key_set(&out), ["active"]);
        assert!(out["active"].is_boolean());
    }

    #[test]
    fn test_safety_panic_taps_json_shape_reports_the_count() {
        let body = json!({ "panic_tap_count": 5 });
        let out = json_value(&body);
        assert_eq!(key_set(&out), ["panic_tap_count"]);
        assert!(out["panic_tap_count"].is_number());
        assert_eq!(panic_taps_line(&body), "panic tap count: 5");
    }
}
