//! `springtale onboarding` — the guided per-platform setup forms, over
//! the daemon. The same forms the dashboard's onboarding wizard renders.

use anyhow::Result;
use serde_json::Value;

use crate::cli::OnboardingAction;
use crate::client::Client;
use crate::commands::json_input;
use crate::output;

/// Handle onboarding subcommands.
pub async fn run(action: OnboardingAction, json_out: bool) -> Result<()> {
    let client = Client::from_config()?;
    match action {
        OnboardingAction::Platforms => {
            let body: Value = client.get("/onboarding/platforms").await?;
            output::emit(json_out, &body, platforms_table)?;
        }
        OnboardingAction::Apply { platform, answers } => {
            let answers = json_input::load(&answers)?;
            let body: Value = client
                .post(
                    &format!("/onboarding/{platform}"),
                    &serde_json::json!({ "answers": answers }),
                )
                .await?;
            output::emit(json_out, &body, |v| {
                serde_json::to_string_pretty(v).unwrap_or_default()
            })?;
        }
    }
    Ok(())
}

/// The `onboarding platforms` table — one row per guided setup form.
fn platforms_table(v: &Value) -> String {
    let rows = output::array(v, "platforms")
        .iter()
        .map(|p| {
            vec![
                output::cell(p, "platform"),
                output::cell(p, "label"),
                output::cell(p, "description"),
            ]
        })
        .collect();
    output::rows_table(&["PLATFORM", "LABEL", "DESCRIPTION"], rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::{json_value, key_set};
    use serde_json::json;

    fn platforms() -> Value {
        json!({
            "platforms": [{
                "platform": "telegram",
                "label": "Telegram",
                "description": "Link a bot token from @BotFather",
            }]
        })
    }

    #[test]
    fn test_onboarding_platforms_json_shape_is_a_platforms_envelope() {
        let out = json_value(&platforms());
        assert_eq!(key_set(&out), ["platforms"]);
        assert!(out["platforms"].is_array());
        let platform = &out["platforms"][0];
        assert!(platform["platform"].is_string());
        assert!(platform["label"].is_string());
        assert!(platform["description"].is_string());
    }

    #[test]
    fn test_platforms_table_reads_every_field_the_json_shape_promises() {
        let table = platforms_table(&platforms());
        for want in ["PLATFORM", "telegram", "Telegram", "@BotFather"] {
            assert!(table.contains(want), "table lost {want}:\n{table}");
        }
    }

    #[test]
    fn test_platforms_table_is_empty_when_none_are_offered() {
        assert_eq!(platforms_table(&json!({ "platforms": [] })), "");
    }
}
