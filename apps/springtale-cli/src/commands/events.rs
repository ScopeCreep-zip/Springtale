//! `springtale events` — the event log, over the daemon.

use anyhow::Result;
use serde_json::Value;

use crate::client::Client;
use crate::output;

/// The event log. Filters are appended to it as a query.
const EVENTS: &str = "/events";

/// Display the event log.
pub async fn run(limit: u32, connector: Option<String>, json_out: bool) -> Result<()> {
    let client = Client::from_config()?;
    let path = match connector {
        Some(name) => format!("{EVENTS}?limit={limit}&connector={name}"),
        None => format!("{EVENTS}?limit={limit}"),
    };
    let body: Value = client.get(&path).await?;
    output::emit(json_out, &body, events_table)
}

/// The `events` table — one row per logged event.
fn events_table(v: &Value) -> String {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::{json_value, key_set};
    use serde_json::json;

    fn log() -> Value {
        json!({
            "events": [{
                "timestamp": "2026-09-04T10:00:00Z",
                "connector_name": "telegram",
                "trigger_type": "message",
                "action_taken": "nightly-digest",
            }]
        })
    }

    #[test]
    fn test_events_json_shape_is_an_events_envelope() {
        let out = json_value(&log());
        assert_eq!(key_set(&out), ["events"]);
        assert!(out["events"].is_array());
        let event = &out["events"][0];
        assert!(event["timestamp"].is_string());
        assert!(event["connector_name"].is_string());
        assert!(event["trigger_type"].is_string());
        assert!(event["action_taken"].is_string());
    }

    #[test]
    fn test_events_table_reads_every_field_the_json_shape_promises() {
        let table = events_table(&log());
        for want in ["TIMESTAMP", "telegram", "message", "nightly-digest"] {
            assert!(table.contains(want), "table lost {want}:\n{table}");
        }
    }

    #[test]
    fn test_events_table_is_empty_for_an_empty_log() {
        assert_eq!(events_table(&json!({ "events": [] })), "");
    }
}
