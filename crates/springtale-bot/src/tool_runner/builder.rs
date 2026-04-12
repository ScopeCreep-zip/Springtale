//! Build the tool list the AI adapter sees from the connector registry.

use std::sync::Arc;

use serde_json::json;
use springtale_ai::ToolDefinition;
use springtale_connector::registry::store::ConnectorRegistry;
use tokio::sync::RwLock;

/// Separator between connector name and action name in a tool name.
///
/// OpenAI's tool-name regex (`^[a-zA-Z0-9_-]{1,64}$`) forbids `.` and
/// `:`, but allows `_` — so we use `__` as a delimiter that survives
/// round-tripping through the model and still reads unambiguously.
pub const TOOL_NAME_SEPARATOR: &str = "__";

/// Enumerate all actions from every enabled connector and return them
/// as tool definitions the adapter can pass to the model.
///
/// Disabled connectors are skipped entirely — the model never sees
/// them, so it can't try to call them and get a "connector disabled"
/// error. Connectors are always exposed via the normalized
/// `{connector}__{action}` name; splitting on [`TOOL_NAME_SEPARATOR`]
/// recovers both halves when a tool call comes back in.
pub async fn collect_tools(registry: &Arc<RwLock<ConnectorRegistry>>) -> Vec<ToolDefinition> {
    let mut tools = Vec::new();
    let reg = registry.read().await;
    for (name, enabled) in reg.list() {
        if !enabled {
            continue;
        }
        let Some(entry) = reg.get(name) else {
            continue;
        };
        let manifest = entry.host.manifest();
        for action in &manifest.actions {
            let tool_name = format!("{name}{TOOL_NAME_SEPARATOR}{}", action.name);
            let description = format!(
                "{} (via {name}). {}",
                action.name, action.description
            );
            let schema = action
                .input_schema
                .clone()
                .unwrap_or_else(|| json!({"type": "object", "properties": {}}));
            tools.push(ToolDefinition {
                name: tool_name,
                description,
                input_schema: schema,
            });
        }
    }
    tools
}

/// Split a tool name produced by [`collect_tools`] back into
/// `(connector_name, action)`. Returns `None` if the separator is
/// missing or the action half is empty.
pub fn split_tool_name(tool_name: &str) -> Option<(&str, &str)> {
    let (connector, action) = tool_name.rsplit_once(TOOL_NAME_SEPARATOR)?;
    if connector.is_empty() || action.is_empty() {
        return None;
    }
    Some((connector, action))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn split_valid_tool_name() {
        let (conn, action) = split_tool_name("connector-telegram__send_message").unwrap();
        assert_eq!(conn, "connector-telegram");
        assert_eq!(action, "send_message");
    }

    #[test]
    fn split_rejects_missing_separator() {
        assert!(split_tool_name("send_message").is_none());
    }

    #[test]
    fn split_rejects_empty_halves() {
        assert!(split_tool_name("__send_message").is_none());
        assert!(split_tool_name("connector__").is_none());
    }
}
