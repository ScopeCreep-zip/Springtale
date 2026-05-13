use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::client::SlackApi;
use crate::error::SlackError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        name: "send_message".to_owned(),
        description: "Send a text message to a Slack channel.".to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "channel": { "type": "string", "description": "Channel ID (C...)" },
                "chat_id": { "type": "string", "description": "Alias for channel (bot response routing)" },
                "text": { "type": "string", "description": "Message text" }
            },
            "required": ["text"]
        })),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "ts": { "type": "string", "description": "Message timestamp (unique ID)" }
            }
        })),
    }
}

pub async fn execute(
    client: &dyn SlackApi,
    input: &serde_json::Value,
) -> Result<ActionResult, SlackError> {
    // Accept both "channel" and "chat_id" (bot response dispatcher
    // sends "chat_id"). D1 — values may also be `slack://channel/{C…}`
    // or `slack://im/{D…}` URIs; the boundary parser unwraps.
    let raw_channel = input
        .get("channel")
        .or_else(|| input.get("chat_id"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| SlackError::InvalidInput("missing 'channel' or 'chat_id'".into()))?;
    let channel = springtale_connector::workspace_key::extract_id_for_scheme(
        raw_channel,
        "connector-slack",
    )
    .map_err(|e| SlackError::InvalidInput(e.to_string()))?;

    let text = input
        .get("text")
        .and_then(|v| v.as_str())
        .ok_or_else(|| SlackError::InvalidInput("missing 'text'".into()))?;

    let response = client.send_message(channel, text).await?;

    let ts = response.get("ts").and_then(|t| t.as_str()).unwrap_or("");

    Ok(ActionResult {
        success: true,
        output: serde_json::json!({ "ts": ts }),
        message: format!("sent message to channel {channel}"),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::client::test_helpers::MockSlackApi;

    #[tokio::test]
    async fn test_execute_send_message_success() {
        let client = MockSlackApi;
        let input = serde_json::json!({ "channel": "C123", "text": "hello" });
        let result = execute(&client, &input).await.unwrap();
        assert!(result.success);
        assert!(result.message.contains("C123"));
    }

    #[tokio::test]
    async fn test_execute_send_message_via_chat_id() {
        let client = MockSlackApi;
        let input = serde_json::json!({ "chat_id": "C456", "text": "hello" });
        let result = execute(&client, &input).await.unwrap();
        assert!(result.success);
        assert!(result.message.contains("C456"));
    }

    #[tokio::test]
    async fn test_execute_send_message_missing_channel() {
        let client = MockSlackApi;
        let input = serde_json::json!({ "text": "hello" });
        let result = execute(&client, &input).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_send_message_missing_text() {
        let client = MockSlackApi;
        let input = serde_json::json!({ "channel": "C123" });
        let result = execute(&client, &input).await;
        assert!(result.is_err());
    }
}
