use std::sync::Arc;

use rmcp::model::{JsonObject, Tool, ToolAnnotations};
use springtale_connector::manifest::types::ActionDecl;

/// Convert a connector's `ActionDecl` into an MCP `Tool` definition.
///
/// The tool is named `{connector}.{action}` because the daemon exposes
/// the whole registry through one MCP endpoint — action names are only
/// unique within a connector.
///
/// The input schema comes from `ActionDecl.input_schema` (JSON Schema
/// from the connector manifest). If no schema is provided, an empty
/// object schema is used.
///
/// The manifest's plan-0.1 hints round-trip into MCP's tool annotations:
/// `read_only` becomes `readOnlyHint`, and `destructive` becomes
/// `destructiveHint` with MCP's own conservative default — unknown
/// (`None`) classifies as destructive, matching the spec's default of
/// `true`. Both are advisory: the sentinel, not the hint, decides.
pub fn action_to_tool(connector: &str, action: &ActionDecl) -> Tool {
    let input_schema = match &action.input_schema {
        Some(serde_json::Value::Object(map)) => Arc::new(map.clone()),
        Some(_non_object) => {
            // Schema is a Value but not an Object — can't use as JsonObject
            tracing::warn!(
                connector = %connector,
                action = %action.name,
                "action input_schema is not a JSON object, using empty schema"
            );
            Arc::new(empty_schema())
        }
        None => Arc::new(empty_schema()),
    };

    let mut tool = Tool::new(
        qualified_name(connector, &action.name),
        action.description.clone(),
        input_schema,
    )
    .annotate(
        ToolAnnotations::new()
            .read_only(action.read_only)
            .destructive(!action.read_only && action.destructive.unwrap_or(true)),
    );

    // Pass through output_schema if the connector declares one
    if let Some(serde_json::Value::Object(map)) = &action.output_schema {
        tool = tool.with_raw_output_schema(Arc::new(map.clone()));
    }

    tool
}

/// Convert all actions of one connector into MCP tools.
pub fn actions_to_tools(connector: &str, actions: &[ActionDecl]) -> Vec<Tool> {
    actions
        .iter()
        .map(|action| action_to_tool(connector, action))
        .collect()
}

/// `{connector}.{action}` — the registry-wide MCP tool name.
pub fn qualified_name(connector: &str, action: &str) -> String {
    format!("{connector}{}{action}", crate::TOOL_NAME_SEPARATOR)
}

/// Empty JSON Schema for actions without declared input parameters.
fn empty_schema() -> JsonObject {
    let mut schema = serde_json::Map::new();
    schema.insert("type".to_owned(), serde_json::json!("object"));
    schema.insert(
        "properties".to_owned(),
        serde_json::Value::Object(serde_json::Map::new()),
    );
    schema
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decl(name: &str, read_only: bool, destructive: Option<bool>) -> ActionDecl {
        ActionDecl {
            read_only,
            destructive,
            poll_interval_secs: None,
            name: name.into(),
            description: format!("{name} description"),
            input_schema: None,
            output_schema: None,
        }
    }

    #[test]
    fn test_action_to_tool_with_schema() {
        let mut action = decl("search", true, None);
        action.description = "Search the web".into();
        action.input_schema = Some(serde_json::json!({
            "type": "object",
            "properties": { "query": { "type": "string" } },
            "required": ["query"]
        }));

        let tool = action_to_tool("connector-web", &action);
        assert_eq!(tool.name.as_ref(), "connector-web.search");
        assert_eq!(
            tool.description.as_ref().map(|d| d.as_ref()),
            Some("Search the web")
        );
        assert!(tool.input_schema.contains_key("properties"));
    }

    #[test]
    fn test_action_to_tool_no_schema() {
        let tool = action_to_tool("c", &decl("ping", false, None));
        assert_eq!(tool.name.as_ref(), "c.ping");
        assert_eq!(
            tool.input_schema.get("type"),
            Some(&serde_json::json!("object"))
        );
    }

    #[test]
    fn test_read_only_action_annotates_read_only_not_destructive() {
        let tool = action_to_tool("c", &decl("list", true, None));
        let ann = tool.annotations.as_ref().expect("annotations present");
        assert_eq!(ann.read_only_hint, Some(true));
        assert_eq!(ann.destructive_hint, Some(false));
    }

    #[test]
    fn test_unknown_destructive_hint_defaults_to_destructive() {
        let tool = action_to_tool("c", &decl("send", false, None));
        let ann = tool.annotations.as_ref().expect("annotations present");
        assert_eq!(ann.read_only_hint, Some(false));
        assert_eq!(ann.destructive_hint, Some(true));
    }

    #[test]
    fn test_declared_non_destructive_hint_round_trips() {
        let tool = action_to_tool("c", &decl("append", false, Some(false)));
        let ann = tool.annotations.as_ref().expect("annotations present");
        assert_eq!(ann.destructive_hint, Some(false));
    }

    #[test]
    fn test_actions_to_tools() {
        let actions = vec![decl("a", false, None), decl("b", true, None)];
        let tools = actions_to_tools("conn", &actions);
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].name.as_ref(), "conn.a");
        assert_eq!(tools[1].name.as_ref(), "conn.b");
    }
}
