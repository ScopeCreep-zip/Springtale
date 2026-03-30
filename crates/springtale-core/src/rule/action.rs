use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Maximum nesting depth for Chain actions.
pub const MAX_CHAIN_DEPTH: u32 = 4;

/// An action to perform when a rule's conditions are met.
///
/// Actions are the "do" part of a rule. They execute sequentially within
/// a pipeline. `Chain` allows multi-step workflows.
///
/// Note: `RunConnector` stores the connector name as a String — springtale-core
/// has no dependency on springtale-connector. The dispatch from action to
/// actual connector call happens in the application layer.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type")]
pub enum Action {
    /// Call a connector action.
    RunConnector {
        /// Connector name (e.g., "connector-kick").
        connector: String,
        /// Action name (e.g., "send_chat").
        action: String,
        /// Parameters passed to the connector action.
        #[serde(default)]
        params: serde_json::Map<String, serde_json::Value>,
    },

    /// Send a message (destination determined by context).
    SendMessage {
        /// Message text (may contain template variables like `${trigger.field}`).
        text: String,
    },

    /// Write to a file.
    WriteFile {
        /// Destination path (may contain template variables).
        destination: String,
        /// Content to write (may contain template variables).
        #[serde(default)]
        content: String,
        /// Whether to delete the source file (for move operations).
        #[serde(default)]
        delete_source: bool,
    },

    /// Execute a shell command (requires ShellExec capability).
    RunShell {
        /// Command to execute.
        command: String,
    },

    /// Send a notification.
    Notify {
        /// Notification title.
        title: String,
        /// Notification body (may contain template variables).
        #[serde(default)]
        body: String,
    },

    /// Execute a sequence of actions as a pipeline.
    Chain {
        /// Ordered list of sub-actions.
        steps: Vec<Action>,
    },

    /// Transform the pipeline data (field extraction, formatting).
    Transform {
        /// Operation: "extract", "format", "filter".
        operation: String,
        /// Operation-specific parameters.
        #[serde(default)]
        params: serde_json::Map<String, serde_json::Value>,
    },

    /// Delay execution for a specified duration.
    Delay {
        /// Delay in seconds.
        seconds: u64,
    },

    /// Call the user's AI adapter (Phase 2a — skipped if NoopAdapter).
    AiComplete {
        /// Prompt text (may contain template variables).
        prompt: String,
        /// Which adapter to use (optional, uses default if omitted).
        #[serde(default)]
        adapter: Option<String>,
    },
}
