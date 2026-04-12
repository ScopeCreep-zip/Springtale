//! Build the tool list the AI adapter sees from the connector registry,
//! filtered by the bot's `ToolPolicy`.

use std::sync::Arc;

use serde_json::json;
use springtale_ai::{ToolDefinition, ToolPolicy, MAX_TOOLS_HARD_CAP, schema_has_secret_fields};
use springtale_connector::registry::store::ConnectorRegistry;
use tokio::sync::RwLock;

/// Separator between connector name and action name in a tool name.
///
/// OpenAI's tool-name regex (`^[a-zA-Z0-9_-]{1,64}$`) forbids `.` and
/// `:`, but allows `_` — so we use `__` as a delimiter that survives
/// round-tripping through the model and still reads unambiguously.
pub const TOOL_NAME_SEPARATOR: &str = "__";

/// Enumerate enabled connector actions filtered by the bot's `ToolPolicy`.
///
/// - Policy `allow` is empty → zero tools (safe default per OWASP LLM06).
/// - Deny overrides allow.
/// - Schemas containing secret-named fields (`*_key`, `*_token`, etc.)
///   are rejected so the model never sees credential shapes.
/// - Hard-capped at `MAX_TOOLS_HARD_CAP` (50) — Anthropic docs report
///   degradation past this threshold.
pub async fn collect_tools(
    registry: &Arc<RwLock<ConnectorRegistry>>,
    policy: &ToolPolicy,
) -> Vec<ToolDefinition> {
    if policy.allow.is_empty() {
        return Vec::new();
    }
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
            if !policy.is_allowed(&tool_name) {
                continue;
            }
            let schema = action
                .input_schema
                .clone()
                .unwrap_or_else(|| json!({"type": "object", "properties": {}}));
            if schema_has_secret_fields(&schema) {
                tracing::warn!(tool = %tool_name, "skipped — schema contains secret fields");
                continue;
            }
            let description = format!(
                "{} (via {name}). {}",
                action.name, action.description
            );
            tools.push(ToolDefinition {
                name: tool_name,
                description,
                input_schema: schema,
            });
        }
    }
    if tools.len() > MAX_TOOLS_HARD_CAP {
        tracing::warn!(
            count = tools.len(),
            cap = MAX_TOOLS_HARD_CAP,
            "tool count exceeds cap, truncating"
        );
        tools.truncate(MAX_TOOLS_HARD_CAP);
    }
    tools
}

/// Split a tool name produced by [`collect_tools`] back into
/// `(connector_name, action)`. Returns `None` if the separator is
/// missing or either half is empty.
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

    #[test]
    fn empty_allow_returns_zero_tools() {
        let policy = ToolPolicy::default();
        assert!(policy.allow.is_empty());
        assert!(!policy.is_allowed("connector-telegram__send_message"));
    }

    #[test]
    fn allow_glob_matches() {
        let policy = ToolPolicy {
            allow: vec!["connector-telegram__*".into()],
            ..Default::default()
        };
        assert!(policy.is_allowed("connector-telegram__send_message"));
        assert!(!policy.is_allowed("connector-shell__execute"));
    }

    #[test]
    fn deny_overrides_allow() {
        let policy = ToolPolicy {
            allow: vec!["*".into()],
            deny: vec!["*__execute".into()],
            ..Default::default()
        };
        assert!(policy.is_allowed("connector-telegram__send_message"));
        assert!(!policy.is_allowed("connector-shell__execute"));
    }

    #[test]
    fn secret_field_detection() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "api_key": { "type": "string" },
                "query": { "type": "string" }
            }
        });
        assert!(schema_has_secret_fields(&schema));

        let clean = serde_json::json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" }
            }
        });
        assert!(!schema_has_secret_fields(&clean));
    }
}
