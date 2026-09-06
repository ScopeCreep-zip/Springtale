use std::sync::Arc;

use rmcp::model::{JsonObject, Tool};
use springtale_connector::manifest::types::ActionDecl;

/// Convert a connector's `ActionDecl` into an MCP `Tool` definition.
///
/// The tool name and description come directly from the action declaration.
/// The input schema comes from `ActionDecl.input_schema` (JSON Schema from
/// the connector manifest). If no schema is provided, an empty object schema
/// is used.
pub fn action_to_tool(action: &ActionDecl) -> Tool {
    let input_schema = match &action.input_schema {
        Some(serde_json::Value::Object(map)) => Arc::new(map.clone()),
        Some(_non_object) => {
            // Schema is a Value but not an Object — can't use as JsonObject
            tracing::warn!(
                action = %action.name,
                "action input_schema is not a JSON object, using empty schema"
            );
            Arc::new(empty_schema())
        }
        None => Arc::new(empty_schema()),
    };

    let mut tool = Tool::new(
        action.name.clone(),
        action.description.clone(),
        input_schema,
    );

    // Pass through output_schema if the connector declares one
    if let Some(serde_json::Value::Object(map)) = &action.output_schema {
        tool = tool.with_raw_output_schema(Arc::new(map.clone()));
    }

    tool
}

/// Convert all actions from a connector into MCP tools.
pub fn actions_to_tools(actions: &[ActionDecl]) -> Vec<Tool> {
    actions.iter().map(action_to_tool).collect()
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

    #[test]
    fn test_action_to_tool_with_schema() {
        let action = ActionDecl {
            read_only: false,
            destructive: None,
            poll_interval_secs: None,
            name: "search".into(),
            description: "Search the web".into(),
            input_schema: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" }
                },
                "required": ["query"]
            })),
            output_schema: None,
        };

        let tool = action_to_tool(&action);
        assert_eq!(tool.name.as_ref(), "search");
        assert_eq!(
            tool.description.as_ref().map(|d| d.as_ref()),
            Some("Search the web")
        );
        // Schema should have the "query" property
        assert!(tool.input_schema.contains_key("properties"));
    }

    #[test]
    fn test_action_to_tool_no_schema() {
        let action = ActionDecl {
            read_only: false,
            destructive: None,
            poll_interval_secs: None,
            name: "ping".into(),
            description: "Ping the service".into(),
            input_schema: None,
            output_schema: None,
        };

        let tool = action_to_tool(&action);
        assert_eq!(tool.name.as_ref(), "ping");
        // Should have empty schema
        assert_eq!(
            tool.input_schema.get("type"),
            Some(&serde_json::json!("object"))
        );
    }

    #[test]
    fn test_actions_to_tools() {
        let actions = vec![
            ActionDecl {
                read_only: false,
                destructive: None,
                poll_interval_secs: None,
                name: "a".into(),
                description: "action a".into(),
                input_schema: None,
                output_schema: None,
            },
            ActionDecl {
                read_only: false,
                destructive: None,
                poll_interval_secs: None,
                name: "b".into(),
                description: "action b".into(),
                input_schema: None,
                output_schema: None,
            },
        ];

        let tools = actions_to_tools(&actions);
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].name.as_ref(), "a");
        assert_eq!(tools[1].name.as_ref(), "b");
    }
}
