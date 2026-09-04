//! Active-discovery action — enumerates every text channel the bot can
//! see across every guild it is a member of.
//!
//! Walks `GET /users/@me/guilds` then `GET /guilds/{id}/channels` and
//! emits one row per `(guild, channel)` pair. The resulting workspace
//! keys are `discord://guild/{guild_id}/channel/{channel_id}`.

use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;
use springtale_connector::workspace_key;

use crate::client::DiscordApi;
use crate::error::DiscordError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        read_only: true,
        destructive: None,
        name: "discover_destinations".to_owned(),
        description:
            "Enumerate every text channel this bot can see across every guild it's a member of."
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
    client: &dyn DiscordApi,
    _input: &serde_json::Value,
) -> Result<ActionResult, DiscordError> {
    let channels = client.list_destinations().await?;
    let mut rows = Vec::with_capacity(channels.len());
    for ch in &channels {
        let g = ch.guild_id.to_string();
        let c = ch.channel_id.to_string();
        let workspace_key = workspace_key::build("discord", &["guild", &g, "channel", &c]);
        let mut metadata = serde_json::Map::new();
        metadata.insert("guild_id".to_owned(), serde_json::Value::String(g));
        metadata.insert(
            "guild_name".to_owned(),
            serde_json::Value::String(ch.guild_name.clone()),
        );
        metadata.insert("channel_id".to_owned(), serde_json::Value::String(c));
        rows.push(serde_json::json!({
            "workspace_key": workspace_key,
            "display_name": format!("{} / #{}", ch.guild_name, ch.channel_name),
            "kind": "channel",
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
    use crate::client::test_helpers::MockDiscordApi;

    #[test]
    fn test_declaration_name() {
        assert_eq!(declaration().name, "discover_destinations");
    }

    #[tokio::test]
    async fn test_execute_returns_six_rows_from_mock_2x3_grid() {
        let mock = MockDiscordApi;
        let result = execute(&mock, &serde_json::json!({})).await.unwrap();
        let arr = result
            .output
            .get("workspaces")
            .and_then(|v| v.as_array())
            .unwrap();
        assert_eq!(arr.len(), 6, "mock should yield 2 guilds × 3 channels");
    }

    #[tokio::test]
    async fn test_execute_keys_use_uri_scheme() {
        let mock = MockDiscordApi;
        let result = execute(&mock, &serde_json::json!({})).await.unwrap();
        let arr = result.output["workspaces"].as_array().unwrap();
        for row in arr {
            let k = row["workspace_key"].as_str().unwrap();
            assert!(k.starts_with("discord://guild/"), "got: {k}");
            assert!(k.contains("/channel/"), "got: {k}");
        }
    }

    #[tokio::test]
    async fn test_execute_metadata_contains_guild_and_channel_ids() {
        let mock = MockDiscordApi;
        let result = execute(&mock, &serde_json::json!({})).await.unwrap();
        let row = &result.output["workspaces"][0];
        let metadata = row["metadata"].as_object().unwrap();
        assert!(metadata.contains_key("guild_id"));
        assert!(metadata.contains_key("guild_name"));
        assert!(metadata.contains_key("channel_id"));
    }

    #[tokio::test]
    async fn test_execute_kind_is_channel() {
        let mock = MockDiscordApi;
        let result = execute(&mock, &serde_json::json!({})).await.unwrap();
        for row in result.output["workspaces"].as_array().unwrap() {
            assert_eq!(row["kind"].as_str().unwrap(), "channel");
        }
    }
}
