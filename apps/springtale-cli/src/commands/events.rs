//! `springtale events` — the event log, over the daemon.

use anyhow::Result;
use serde_json::Value;

use crate::client::Client;
use crate::output;

/// Display the event log.
pub async fn run(limit: u32, connector: Option<String>, json_out: bool) -> Result<()> {
    let client = Client::from_config()?;
    let path = match connector {
        Some(name) => format!("/events?limit={limit}&connector={name}"),
        None => format!("/events?limit={limit}"),
    };
    let body: Value = client.get(&path).await?;
    output::emit(json_out, &body, |v| {
        let rows = output::array(v, "events")
            .iter()
            .map(|e| {
                vec![
                    output::cell(e, "timestamp"),
                    output::cell(e, "connector_name"),
                    output::cell(e, "trigger_type"),
                    output::cell(e, "action_taken"),
                ]
            })
            .collect();
        output::rows_table(&["TIMESTAMP", "CONNECTOR", "TRIGGER", "ACTION"], rows)
    })
}
