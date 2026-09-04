use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::client::DiscordApi;
use crate::error::DiscordError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        read_only: false,
        destructive: None,
        name: "send_embed".to_owned(),
        description: "Send a rich embed to a Discord channel.".to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "channel_id": { "type": "string", "description": "Target channel ID" },
                "title": { "type": "string", "description": "Embed title" },
                "description": { "type": "string", "description": "Embed description" },
                "color": { "type": "integer", "description": "Embed color (decimal)" },
                "fields": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string" },
                            "value": { "type": "string" },
                            "inline": { "type": "boolean" }
                        },
                        "required": ["name", "value"]
                    }
                }
            },
            "required": ["channel_id"]
        })),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "message": { "type": "object" }
            }
        })),
    }
}

pub async fn execute(
    client: &dyn DiscordApi,
    input: &serde_json::Value,
) -> Result<ActionResult, DiscordError> {
    let channel_id: u64 = input
        .get("channel_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| DiscordError::InvalidInput("missing 'channel_id'".into()))?
        .parse()
        .map_err(|_| DiscordError::InvalidInput("invalid channel_id".into()))?;

    let embed = serde_json::json!({
        "title": input.get("title").and_then(|t| t.as_str()).unwrap_or(""),
        "description": input.get("description").and_then(|d| d.as_str()),
        "color": input.get("color").and_then(|c| c.as_u64()).unwrap_or(0),
        "fields": input.get("fields").cloned().unwrap_or(serde_json::json!([])),
    });

    let response = client.send_embed(channel_id, embed).await?;

    Ok(ActionResult {
        success: true,
        output: serde_json::json!({ "message": response }),
        message: format!("sent embed to channel {channel_id}"),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::client::test_helpers::MockDiscordApi;

    #[tokio::test]
    async fn test_execute_send_embed_success() {
        let client = MockDiscordApi;
        let input = serde_json::json!({
            "channel_id": "123456789",
            "title": "Test Embed",
            "description": "A test embed",
            "color": 0xFF0000
        });

        let result = execute(&client, &input).await.unwrap();
        assert!(result.success);
        assert!(result.message.contains("embed"));
    }

    #[tokio::test]
    async fn test_execute_send_embed_missing_channel() {
        let client = MockDiscordApi;
        let input = serde_json::json!({ "title": "Test" });

        let result = execute(&client, &input).await;
        assert!(result.is_err());
    }
}
