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
            output::emit(json_out, &body, |v| {
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
            })?;
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
