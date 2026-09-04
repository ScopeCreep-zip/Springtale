//! Active-discovery action — returns the in-memory cache of channels
//! the bot has joined this session plus nicks it has DM'd.
//!
//! We deliberately do NOT issue a network-wide `LIST` — that would
//! flood the connector with every channel on the network (a privacy +
//! bandwidth hazard). Discovery here is the bot's own footprint, not a
//! global directory.

use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;
use springtale_connector::workspace_key;

use crate::client::IrcApi;
use crate::error::IrcError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        read_only: true,
        destructive: None,
        name: "discover_destinations".to_owned(),
        description:
            "Enumerate channels this bot has joined + nicks it has DM'd this session (no network-wide LIST)."
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
    client: &dyn IrcApi,
    _input: &serde_json::Value,
) -> Result<ActionResult, IrcError> {
    let targets = client.list_destinations().await?;
    let mut rows = Vec::with_capacity(targets.len());
    for t in &targets {
        let segment = match t.kind.as_str() {
            "channel" => "channel",
            _ => "user",
        };
        let workspace_key = workspace_key::build("irc", &["network", &t.network, segment, &t.id]);
        let mut metadata = serde_json::Map::new();
        metadata.insert(
            "network".to_owned(),
            serde_json::Value::String(t.network.clone()),
        );
        metadata.insert("name".to_owned(), serde_json::Value::String(t.id.clone()));
        rows.push(serde_json::json!({
            "workspace_key": workspace_key,
            "display_name": t.id.clone(),
            "kind": t.kind.clone(),
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
    use crate::client::test_helpers::MockIrcApi;

    #[test]
    fn test_declaration_name() {
        assert_eq!(declaration().name, "discover_destinations");
    }

    #[tokio::test]
    async fn test_execute_returns_channels_and_users() {
        let mock = MockIrcApi;
        let result = execute(&mock, &serde_json::json!({})).await.unwrap();
        let arr = result.output["workspaces"].as_array().unwrap();
        assert_eq!(arr.len(), 3);
        let kinds: Vec<&str> = arr.iter().map(|r| r["kind"].as_str().unwrap()).collect();
        assert!(kinds.contains(&"channel"));
        assert!(kinds.contains(&"user"));
    }

    #[tokio::test]
    async fn test_execute_channel_uri_includes_network_and_channel_name() {
        let mock = MockIrcApi;
        let result = execute(&mock, &serde_json::json!({})).await.unwrap();
        let chan = result.output["workspaces"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["kind"] == "channel")
            .unwrap();
        let k = chan["workspace_key"].as_str().unwrap();
        assert!(k.starts_with("irc://network/"), "got: {k}");
        assert!(k.contains("/channel/"), "got: {k}");
    }

    #[tokio::test]
    async fn test_execute_user_uri_uses_user_segment() {
        let mock = MockIrcApi;
        let result = execute(&mock, &serde_json::json!({})).await.unwrap();
        let user = result.output["workspaces"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["kind"] == "user")
            .unwrap();
        assert!(
            user["workspace_key"].as_str().unwrap().contains("/user/"),
            "got: {}",
            user["workspace_key"]
        );
    }
}
