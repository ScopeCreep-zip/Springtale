use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::client::DiscordApi;
use crate::error::DiscordError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        read_only: false,
        name: "send_message".to_owned(),
        description: "Send a text message to a Discord channel.".to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "channel_id": { "type": "string", "description": "Target channel ID" },
                "content": { "type": "string", "description": "Message text content" }
            },
            "required": ["channel_id", "content"]
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
    let raw_channel = input
        .get("channel_id")
        .or_else(|| input.get("chat_id")) // bot response compat
        .and_then(|v| v.as_str())
        .ok_or_else(|| DiscordError::InvalidInput("missing 'channel_id'".into()))?;
    // D1 — accept raw u64 or WorkspaceKey URI. The URI's last
    // segment is the channel id; `extract_id_for_scheme` does
    // raw-id fallback when there's no `://`.
    let resolved = springtale_connector::workspace_key::extract_id_for_scheme(
        raw_channel,
        "connector-discord",
    )
    .map_err(|e| DiscordError::InvalidInput(e.to_string()))?;
    let channel_id: u64 = resolved
        .parse()
        .map_err(|_| DiscordError::InvalidInput("invalid channel_id".into()))?;

    let content = input
        .get("content")
        .or_else(|| input.get("text")) // bot response compat
        .and_then(|v| v.as_str())
        .ok_or_else(|| DiscordError::InvalidInput("missing 'content'".into()))?;

    let response = client.send_message(channel_id, content).await?;

    Ok(ActionResult {
        success: true,
        output: serde_json::json!({ "message": response }),
        message: format!("sent message to channel {channel_id}"),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::client::test_helpers::MockDiscordApi;

    #[tokio::test]
    async fn test_execute_send_message_success() {
        let client = MockDiscordApi;
        let input = serde_json::json!({
            "channel_id": "123456789",
            "content": "hello world"
        });

        let result = execute(&client, &input).await.unwrap();
        assert!(result.success);
        assert!(result.message.contains("123456789"));
    }

    #[tokio::test]
    async fn test_execute_send_message_missing_channel() {
        let client = MockDiscordApi;
        let input = serde_json::json!({ "content": "hello" });

        let result = execute(&client, &input).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("channel_id"));
    }

    #[tokio::test]
    async fn test_execute_send_message_missing_content() {
        let client = MockDiscordApi;
        let input = serde_json::json!({ "channel_id": "123" });

        let result = execute(&client, &input).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("content"));
    }
}
