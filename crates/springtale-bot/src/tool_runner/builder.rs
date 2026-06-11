//! Build the tool list the AI adapter sees from the connector registry,
//! filtered by the bot's `ToolPolicy`.

use std::sync::Arc;

use serde_json::json;
use springtale_ai::{MAX_TOOLS_HARD_CAP, ToolDefinition, ToolPolicy, schema_has_secret_fields};
use springtale_connector::registry::store::ConnectorRegistry;
use tokio::sync::RwLock;

/// Separator between connector name and action name in a tool name.
///
/// OpenAI's tool-name regex (`^[a-zA-Z0-9_-]{1,64}$`) forbids `.` and
/// `:`, but allows `_` — so we use `__` as a delimiter that survives
/// round-tripping through the model and still reads unambiguously.
pub const TOOL_NAME_SEPARATOR: &str = "__";

/// Decide whether one connector action is exposed to the model.
///
/// - **Explicit mode** (`allow` non-empty): exactly the allow-list
///   (`ToolPolicy::is_allowed`), as before.
/// - **Default (bimbo) mode** (`allow` empty): every `read_only` action is
///   chat-callable out of the box — zero side effects keeps the OWASP LLM06
///   least-privilege posture while making a fresh install useful. Mutating
///   actions join only when `writes_with_approval` is set (W2 wires the
///   approval gate that makes that safe).
/// - `deny` always wins in both modes.
pub fn tool_permitted(policy: &ToolPolicy, tool_name: &str, read_only: bool) -> bool {
    if !policy.allow.is_empty() {
        return policy.is_allowed(tool_name);
    }
    if policy.is_denied(tool_name) {
        return false;
    }
    read_only || policy.writes_with_approval
}

/// Enumerate enabled connector actions filtered by the bot's `ToolPolicy`.
///
/// - Exposure per action decided by [`tool_permitted`] (default mode =
///   read-only actions out of the box; explicit allow-list unchanged).
/// - Schemas containing secret-named fields (`*_key`, `*_token`, etc.)
///   are rejected so the model never sees credential shapes.
/// - Hard-capped at `MAX_TOOLS_HARD_CAP` (50) — Anthropic docs report
///   degradation past this threshold.
pub async fn collect_tools(
    registry: &Arc<RwLock<ConnectorRegistry>>,
    policy: &ToolPolicy,
) -> Vec<ToolDefinition> {
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
            if !tool_permitted(policy, &tool_name, action.read_only) {
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
            let description = format!("{} (via {name}). {}", action.name, action.description);
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
    fn default_mode_exposes_read_only_only() {
        // Bimbo default: no allow-list ⇒ read-only actions are chat-callable,
        // mutating actions are NOT (until writes_with_approval + the gate).
        let policy = ToolPolicy::default();
        assert!(policy.allow.is_empty());
        assert!(tool_permitted(&policy, "connector-kick__get_channel", true));
        assert!(!tool_permitted(
            &policy,
            "connector-telegram__send_message",
            false
        ));
    }

    #[test]
    fn default_mode_writes_join_only_with_approval_flag() {
        let policy = ToolPolicy {
            writes_with_approval: true,
            ..Default::default()
        };
        assert!(tool_permitted(
            &policy,
            "connector-telegram__send_message",
            false
        ));
    }

    #[test]
    fn default_mode_deny_still_wins() {
        let policy = ToolPolicy {
            deny: vec!["connector-presearch__*".into()],
            ..Default::default()
        };
        assert!(!tool_permitted(
            &policy,
            "connector-presearch__search",
            true
        ));
        assert!(tool_permitted(&policy, "connector-kick__get_stream", true));
    }

    #[test]
    fn explicit_mode_unchanged() {
        // A non-empty allow-list behaves exactly as before — read_only is
        // irrelevant; only the lists decide.
        let policy = ToolPolicy {
            allow: vec!["connector-telegram__*".into()],
            ..Default::default()
        };
        assert!(tool_permitted(
            &policy,
            "connector-telegram__send_message",
            false
        ));
        assert!(!tool_permitted(
            &policy,
            "connector-kick__get_channel",
            true
        ));
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
