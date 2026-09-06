//! `springtale formation` — the orchestration surface, over the daemon.
//!
//! Four verb groups and nothing else: **composition** (add/remove
//! members), **intent** (set or cycle), **constraints** (guard,
//! autonomy), **intervention** (rally, deploy, pause, resume,
//! dissolve), plus read-only inspection. There is no `assign` verb and
//! no route that hands work to a named member — the drum rule.

use anyhow::Result;
use serde_json::{Value, json};

use crate::cli::FormationAction;
use crate::client::Client;
use crate::output;

/// Handle formation subcommands.
pub async fn run(action: FormationAction, json_out: bool) -> Result<()> {
    let client = Client::from_config()?;
    match action {
        FormationAction::List => {
            let body: Value = client.get("/formations").await?;
            output::emit(json_out, &body, |v| {
                let rows = output::array(v, "formations")
                    .iter()
                    .map(|f| {
                        vec![
                            output::cell(f, "id"),
                            output::cell(f, "name"),
                            output::cell(f, "intent"),
                            output::cell(f, "momentum"),
                        ]
                    })
                    .collect();
                output::rows_table(&["ID", "NAME", "INTENT", "MOMENTUM"], rows)
            })?;
        }
        FormationAction::Get { id } => {
            let body: Value = client.get(&format!("/formations/{id}")).await?;
            output::emit(json_out, &body, |v| {
                serde_json::to_string_pretty(v).unwrap_or_default()
            })?;
        }
        FormationAction::Commands { id } => {
            let body: Value = client.get(&format!("/formations/{id}/commands")).await?;
            output::emit(json_out, &body, |v| {
                let rows = output::array(v, "commands")
                    .iter()
                    .map(|c| {
                        vec![
                            output::cell(c, "id"),
                            output::cell(c, "label"),
                            output::cell(c, "enabled"),
                        ]
                    })
                    .collect();
                output::rows_table(&["ID", "LABEL", "ENABLED"], rows)
            })?;
        }
        FormationAction::Intents => {
            let body: Value = client.get("/formations/intents").await?;
            output::emit(json_out, &body, |v| {
                let rows = output::array(v, "intents")
                    .iter()
                    .map(|i| vec![output::cell(i, "value"), output::cell(i, "label")])
                    .collect();
                output::rows_table(&["VALUE", "LABEL"], rows)
            })?;
        }
        FormationAction::Eligible { id } => {
            let body: Value = client
                .get(&format!("/formations/{id}/members/eligible"))
                .await?;
            output::emit(json_out, &body, |v| {
                let rows = output::array(v, "members")
                    .iter()
                    .map(|m| vec![output::cell(m, "name"), output::cell(m, "kind")])
                    .collect();
                output::rows_table(&["NAME", "KIND"], rows)
            })?;
        }
        FormationAction::ProposeIntent { id, intent } => {
            let body: Value = client
                .post(
                    &format!("/formations/{id}/propose-intent"),
                    &json!({ "intent": intent }),
                )
                .await?;
            output::emit(json_out, &body, |v| format!("proposed: {v}"))?;
        }
        FormationAction::Vote { id, vote, choice } => {
            let body: Value = client
                .post(
                    &format!("/formations/{id}/votes/{vote}"),
                    &json!({ "choice": choice }),
                )
                .await?;
            output::emit(json_out, &body, |v| format!("vote recorded: {v}"))?;
        }
        FormationAction::Run { id, command, params } => {
            let params = match params {
                Some(path) => crate::commands::json_input::load(&path)?,
                None => json!({}),
            };
            let body: Value = client
                .post(
                    &format!("/formations/{id}/run-command"),
                    &json!({ "command_id": command, "params": params }),
                )
                .await?;
            output::emit(json_out, &body, |v| format!("ran {command}: {v}"))?;
        }
        FormationAction::DeployTeam { file } => {
            let text = std::fs::read_to_string(&file)
                .map_err(|e| anyhow::anyhow!("failed to read {}: {e}", file.display()))?;
            let team: Value = serde_json::from_str(&text)
                .map_err(|e| anyhow::anyhow!("team spec must be JSON: {e}"))?;
            let body: Value = client.post("/formations/deploy-team", &team).await?;
            output::emit(json_out, &body, |v| format!("deployed team: {v}"))?;
        }
        FormationAction::Deploy { id } => {
            simple(&client, json_out, &format!("/formations/{id}/deploy")).await?
        }
        FormationAction::Pause { id } => {
            simple(&client, json_out, &format!("/formations/{id}/pause")).await?
        }
        FormationAction::Resume { id } => {
            simple(&client, json_out, &format!("/formations/{id}/resume")).await?
        }
        FormationAction::Dissolve { id } => {
            simple(&client, json_out, &format!("/formations/{id}/dissolve")).await?
        }
        FormationAction::Rally { id } => {
            let body: Value = client
                .post(&format!("/formations/{id}/rally"), &json!({}))
                .await?;
            output::emit(json_out, &body, |_| "rally sent".to_owned())?;
        }
        FormationAction::Intent { id, set } => {
            let body: Value = match set {
                Some(intent) => {
                    client
                        .put(
                            &format!("/formations/{id}/intent"),
                            &json!({ "intent": intent }),
                        )
                        .await?
                }
                None => {
                    client
                        .post(&format!("/formations/{id}/cycle-intent"), &json!({}))
                        .await?
                }
            };
            output::emit(json_out, &body, |v| format!("intent: {v}"))?;
        }
        FormationAction::Guard { id } => {
            let body: Value = client
                .post(&format!("/formations/{id}/toggle-guard"), &json!({}))
                .await?;
            output::emit(json_out, &body, |v| format!("guard: {v}"))?;
        }
        FormationAction::Autonomy { id } => {
            let body: Value = client
                .post(&format!("/formations/{id}/cycle-autonomy"), &json!({}))
                .await?;
            output::emit(json_out, &body, |v| format!("autonomy: {v}"))?;
        }
        FormationAction::AddMember { id, connector } => {
            let body: Value = client
                .post(
                    &format!("/formations/{id}/members"),
                    &json!({ "connector_name": connector }),
                )
                .await?;
            output::emit(json_out, &body, |_| format!("added {connector} to {id}"))?;
        }
        FormationAction::RmMember { id, connector } => {
            let body: Value = client
                .delete_with(
                    &format!("/formations/{id}/members"),
                    &json!({ "connector_name": connector }),
                )
                .await?;
            output::emit(json_out, &body, |_| {
                format!("removed {connector} from {id}")
            })?;
        }
    }
    Ok(())
}

/// POST a verb with no body and echo the daemon's acknowledgement.
async fn simple(client: &Client, json_out: bool, path: &str) -> Result<()> {
    let body: Value = client.post(path, &json!({})).await?;
    output::emit(json_out, &body, |v| v.to_string())
}
