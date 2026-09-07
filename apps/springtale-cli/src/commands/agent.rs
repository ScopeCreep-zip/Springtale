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
        AgentAction::States => {
            let body: Value = client.get("/agents/states").await?;
            output::emit(json_out, &body, agents_table)?;
        }
        AgentAction::StepAutonomy { name, direction } => {
            let body: Value = client
                .post(
                    &format!("/agents/{name}/autonomy/step"),
                    &json!({ "direction": direction }),
                )
                .await?;
            output::emit(json_out, &body, |v| {
                format!(
                    "Agent '{name}' autonomy is now: {}",
                    output::cell(v, "level")
                )
            })?;
        }
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

/// The `agent states` table — one row per agent the daemon reports.
fn agents_table(v: &Value) -> String {
    let rows = output::array(v, "agents")
        .iter()
        .map(|a| {
            vec![
                output::cell(a, "name"),
                output::cell(a, "activity"),
                output::cell(a, "autonomy"),
                output::cell(a, "connector_name"),
            ]
        })
        .collect();
    output::rows_table(&["NAME", "ACTIVITY", "AUTONOMY", "CONNECTOR"], rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::{json_value, key_set};

    /// A `GET /agents/states` body, as the daemon answers it.
    fn states() -> Value {
        json!({
            "agents": [{
                "name": "nightly-digest",
                "activity": "firing",
                "autonomy": "suggest",
                "connector_name": "telegram",
            }]
        })
    }

    #[test]
    fn test_agent_states_json_shape_is_an_agents_envelope() {
        let out = json_value(&states());
        assert_eq!(key_set(&out), ["agents"]);
        assert!(out["agents"].is_array());
        let agent = &out["agents"][0];
        assert!(agent["name"].is_string());
        assert!(agent["activity"].is_string());
        assert!(agent["autonomy"].is_string());
        assert!(agent["connector_name"].is_string());
    }

    #[test]
    fn test_agents_table_reads_every_field_the_json_shape_promises() {
        let table = agents_table(&states());
        for want in ["NAME", "nightly-digest", "firing", "suggest", "telegram"] {
            assert!(table.contains(want), "table lost {want}:\n{table}");
        }
    }

    #[test]
    fn test_agents_table_is_empty_for_an_empty_roster() {
        assert_eq!(agents_table(&json!({ "agents": [] })), "");
    }

    #[test]
    fn test_agent_autonomy_json_shape_carries_the_new_level() {
        // `agent step-autonomy` / `set-autonomy` echo the daemon ack.
        let out = json_value(&json!({ "level": "approve" }));
        assert_eq!(key_set(&out), ["level"]);
        assert!(out["level"].is_string());
    }
}
