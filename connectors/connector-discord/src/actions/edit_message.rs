use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::client::DiscordApi;
use crate::error::DiscordError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        read_only: false,
        destructive: None,
        name: "edit_message".to_owned(),
        description: "Edit an existing Discord message.".to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "channel_id": { "type": "string", "description": "Channel containing the message" },
                "message_id": { "type": "string", "description": "ID of the message to edit" },
                "content": { "type": "string", "description": "New message content" }
            },
            "required": ["channel_id", "message_id", "content"]
        })),
        output_schema: None,
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

    let message_id: u64 = input
        .get("message_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| DiscordError::InvalidInput("missing 'message_id'".into()))?
        .parse()
        .map_err(|_| DiscordError::InvalidInput("invalid message_id".into()))?;

    let content = input
        .get("content")
        .and_then(|v| v.as_str())
        .ok_or_else(|| DiscordError::InvalidInput("missing 'content'".into()))?;

    client.edit_message(channel_id, message_id, content).await?;

    Ok(ActionResult {
        success: true,
        output: serde_json::json!({}),
        message: format!("edited message {message_id} in channel {channel_id}"),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::client::test_helpers::MockDiscordApi;

    #[tokio::test]
    async fn test_execute_edit_message_success() {
        let client = MockDiscordApi;
        let input = serde_json::json!({
            "channel_id": "123",
            "message_id": "456",
            "content": "updated content"
        });

        let result = execute(&client, &input).await.unwrap();
        assert!(result.success);
        assert!(result.message.contains("edited"));
    }

    #[tokio::test]
    async fn test_execute_edit_message_missing_fields() {
        let client = MockDiscordApi;

        let result = execute(&client, &serde_json::json!({})).await;
        assert!(result.is_err());
    }
}
