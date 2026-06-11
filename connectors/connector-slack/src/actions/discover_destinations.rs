//! Active-discovery action — enumerates every conversation this bot has
//! access to via Slack's cursor-paginated `conversations.list`.
//!
//! Public channels, private channels, IMs, and mpims are all included.
//! The Slack channel ID's first character determines the URI segment:
//! `C` → `channel`, `G` → `private_channel`, `D` → `im`, `M` → `group`.
//! Output rows are uniform `slack://channel/{C…}` or `slack://im/{D…}`
//! workspace keys.

use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;
use springtale_connector::workspace_key;

use crate::client::SlackApi;
use crate::error::SlackError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        read_only: true,
        name: "discover_destinations".to_owned(),
        description:
            "Enumerate every conversation this bot has access to via Slack's `conversations.list`."
                .to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {}
        })),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "workspaces": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "workspace_key": { "type": "string" },
                            "display_name":  { "type": "string" },
                            "kind":          { "type": "string" },
                            "metadata":      { "type": "object" }
                        }
                    }
                }
            }
        })),
    }
}

pub async fn execute(
    client: &dyn SlackApi,
    _input: &serde_json::Value,
) -> Result<ActionResult, SlackError> {
    let conversations = client.list_destinations().await?;
    let mut rows = Vec::with_capacity(conversations.len());
    for c in &conversations {
        let (segment, kind) = match c.id.chars().next() {
            Some('C') => ("channel", "channel"),
            Some('G') => ("channel", "private_channel"),
            Some('D') => ("im", "dm"),
            Some('M') => ("channel", "group"),
            _ => ("channel", "channel"),
        };
        let workspace_key = workspace_key::build("slack", &[segment, &c.id]);
        let display = match (&c.name, kind) {
            (Some(n), _) => format!("#{n}"),
            (None, "dm") => format!("DM {}", c.id),
            (None, _) => c.id.clone(),
        };
        let mut metadata = serde_json::Map::new();
        metadata.insert(
            "channel_id".to_owned(),
            serde_json::Value::String(c.id.clone()),
        );
        if let Some(n) = &c.name {
            metadata.insert("name".to_owned(), serde_json::Value::String(n.clone()));
        }
        if let Some(m) = c.num_members {
            metadata.insert("num_members".to_owned(), serde_json::Value::from(m));
        }
        metadata.insert(
            "is_private".to_owned(),
            serde_json::Value::Bool(c.is_private),
        );
        rows.push(serde_json::json!({
            "workspace_key": workspace_key,
            "display_name": display,
            "kind": kind,
            "metadata": serde_json::Value::Object(metadata),
        }));
    }
    let count = rows.len();
    Ok(ActionResult {
        success: true,
        output: serde_json::json!({ "workspaces": rows }),
        message: format!("discovered {count} destination(s)"),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::client::test_helpers::MockSlackApi;

    #[test]
    fn test_declaration_name() {
        assert_eq!(declaration().name, "discover_destinations");
    }

    #[tokio::test]
    async fn test_execute_returns_five_rows_covering_all_kinds() {
        let mock = MockSlackApi;
        let result = execute(&mock, &serde_json::json!({})).await.unwrap();
        let arr = result.output["workspaces"].as_array().unwrap();
        assert_eq!(arr.len(), 5);
        let kinds: Vec<&str> = arr.iter().map(|r| r["kind"].as_str().unwrap()).collect();
        assert!(kinds.contains(&"channel"));
        assert!(kinds.contains(&"private_channel"));
        assert!(kinds.contains(&"dm"));
        assert!(kinds.contains(&"group"));
    }

    #[tokio::test]
    async fn test_execute_dm_uses_im_uri_segment() {
        let mock = MockSlackApi;
        let result = execute(&mock, &serde_json::json!({})).await.unwrap();
        let dm = result.output["workspaces"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["kind"] == "dm")
            .unwrap();
        assert!(
            dm["workspace_key"]
                .as_str()
                .unwrap()
                .starts_with("slack://im/")
        );
    }

    #[tokio::test]
    async fn test_execute_channel_uses_channel_uri_segment() {
        let mock = MockSlackApi;
        let result = execute(&mock, &serde_json::json!({})).await.unwrap();
        let chan = result.output["workspaces"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["kind"] == "channel")
            .unwrap();
        assert!(
            chan["workspace_key"]
                .as_str()
                .unwrap()
                .starts_with("slack://channel/")
        );
    }
}
