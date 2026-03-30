use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::client::KickApi;
use crate::error::KickError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        name: "send_chat".to_owned(),
        description: "Send a chat message to a Kick channel.".to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "channel_id": { "type": "string", "description": "Channel ID to send the message to." },
                "message": { "type": "string", "description": "Message content." }
            },
            "required": ["channel_id", "message"]
        })),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "response": { "type": "object", "description": "API response." }
            }
        })),
    }
}

pub async fn execute(
    client: &dyn KickApi,
    input: &serde_json::Value,
) -> Result<ActionResult, KickError> {
    let channel_id = input.get("channel_id").and_then(|v| v.as_str())
        .ok_or_else(|| KickError::InvalidInput("missing 'channel_id'".to_owned()))?;
    let message = input.get("message").and_then(|v| v.as_str())
        .ok_or_else(|| KickError::InvalidInput("missing 'message'".to_owned()))?;

    let response = client.send_chat(channel_id, message).await?;

    Ok(ActionResult {
        success: true,
        output: serde_json::json!({ "response": response }),
        message: format!("sent chat to channel {channel_id}"),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::client::KickClient;

    fn test_client() -> KickClient {
        KickClient::new("http://localhost:0", secrecy::SecretBox::new(Box::new("fake_token".to_owned()))).unwrap()
    }

    #[test]
    fn test_declaration() {
        let decl = declaration();
        assert_eq!(decl.name, "send_chat");
    }

    #[tokio::test]
    async fn test_execute_missing_channel_id_returns_invalid_input() {
        let client = test_client();
        let input = serde_json::json!({ "message": "hello" });
        let result = execute(&client, &input).await;
        assert!(matches!(result, Err(KickError::InvalidInput(_))));
    }

    #[tokio::test]
    async fn test_execute_missing_message_returns_invalid_input() {
        let client = test_client();
        let input = serde_json::json!({ "channel_id": "123" });
        let result = execute(&client, &input).await;
        assert!(matches!(result, Err(KickError::InvalidInput(_))));
    }

    #[tokio::test]
    async fn test_execute_empty_input_returns_invalid_input() {
        let client = test_client();
        let input = serde_json::json!({});
        let result = execute(&client, &input).await;
        assert!(matches!(result, Err(KickError::InvalidInput(_))));
    }

    #[tokio::test]
    async fn test_execute_null_channel_id_returns_invalid_input() {
        let client = test_client();
        let input = serde_json::json!({ "channel_id": null, "message": "hello" });
        let result = execute(&client, &input).await;
        assert!(matches!(result, Err(KickError::InvalidInput(_))));
    }

    #[tokio::test]
    async fn test_execute_null_message_returns_invalid_input() {
        let client = test_client();
        let input = serde_json::json!({ "channel_id": "123", "message": null });
        let result = execute(&client, &input).await;
        assert!(matches!(result, Err(KickError::InvalidInput(_))));
    }

    use crate::client::test_helpers::MockKickApi;

    #[tokio::test]
    async fn test_execute_send_chat_mock_extracts_response() {
        let mock = MockKickApi {
            response: serde_json::json!({
                "data": {
                    "is_sent": true,
                    "message_id": "msg_abc123"
                }
            }),
        };
        let input = serde_json::json!({ "channel_id": "42", "message": "hello kick" });
        let result = execute(&mock, &input).await.unwrap();
        assert!(result.success);
        assert_eq!(result.output["response"]["data"]["is_sent"], true);
        assert_eq!(result.output["response"]["data"]["message_id"], "msg_abc123");
        assert!(result.message.contains("42"));
    }
}
