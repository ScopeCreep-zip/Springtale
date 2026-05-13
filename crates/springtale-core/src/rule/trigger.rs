use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// What event causes a rule to be evaluated.
///
/// No `specta::Type` derive — Trigger is nested in `Rule`, which
/// itself is not a typed Tauri command parameter (rule payloads
/// transit as `serde_json::Value`; the rule builder reads
/// `get_rule_schema()` for shape). See `Action` for the policy.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type")]
pub enum Trigger {
    /// Fire on a cron schedule.
    Cron {
        /// Cron expression (e.g., "0 9 * * *" for 9am daily).
        expression: String,
    },

    /// Fire when a file changes in a watched directory.
    FileWatch {
        /// Path to watch.
        path: String,
        /// Event type: "create", "modify", "delete", or "any".
        #[serde(default = "default_file_event")]
        event: String,
    },

    /// Fire when an inbound webhook is received.
    Webhook {
        /// Path suffix for the webhook endpoint (e.g., "my-hook").
        path: String,
    },

    /// Fire when a connector emits an event.
    ConnectorEvent {
        /// Name of the connector (e.g., "connector-kick").
        connector: String,
        /// Event name (e.g., "stream_live", "chat_message").
        event: String,
    },

    /// Fire on internal system events (startup, shutdown, health check).
    SystemEvent {
        /// Event name: "startup", "shutdown", "health_check".
        event: String,
    },
}

fn default_file_event() -> String {
    "any".to_owned()
}

impl Trigger {
    /// Returns the trigger type as a string for matching.
    pub fn trigger_type(&self) -> &str {
        match self {
            Trigger::Cron { .. } => "Cron",
            Trigger::FileWatch { .. } => "FileWatch",
            Trigger::Webhook { .. } => "Webhook",
            Trigger::ConnectorEvent { .. } => "ConnectorEvent",
            Trigger::SystemEvent { .. } => "SystemEvent",
        }
    }

    /// Returns the connector name if this is a ConnectorEvent trigger.
    pub fn connector_name(&self) -> Option<String> {
        match self {
            Trigger::ConnectorEvent { connector, .. } => Some(connector.clone()),
            _ => None,
        }
    }
}
