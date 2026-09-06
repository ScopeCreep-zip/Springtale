use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::client::TelegramApi;
use crate::error::TelegramError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        read_only: false,
        destructive: None,
        poll_interval_secs: None,
        name: "send_message".to_owned(),
        description: "Send a text message to a Telegram chat.".to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "chat_id": { "type": "string", "description": "Chat ID or @username." },
                "text": { "type": "string", "description": "Message text (up to 4096 chars)." },
                "parse_mode": { "type": "string", "enum": ["HTML", "Markdown", "MarkdownV2"] },
                "reply_to_message_id": { "type": "integer" }
            },
            "required": ["chat_id", "text"]
        })),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": { "message": { "type": "object" } }
        })),
    }
}

pub async fn execute(
    client: &dyn TelegramApi,
    input: &serde_json::Value,
) -> Result<ActionResult, TelegramError> {
    let raw_chat_id = input
        .get("chat_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| TelegramError::InvalidInput("missing 'chat_id'".to_owned()))?;
    // D1 — accept either a raw chat id (`"12345"`, `"@channel"`) or
    // a `WorkspaceKey` URI (`"telegram://chat/12345"`). The parser
    // extracts the raw id when the input is a URI matching this
    // connector's scheme; falls back to raw-id semantics
    // otherwise. Mismatched-scheme URIs surface a clear error.
    let chat_id = springtale_connector::workspace_key::extract_id_for_scheme(
        raw_chat_id,
        "connector-telegram",
    )
    .map_err(|e| TelegramError::InvalidInput(e.to_string()))?;
    let text = input
        .get("text")
        .and_then(|v| v.as_str())
        .ok_or_else(|| TelegramError::InvalidInput("missing 'text'".to_owned()))?;
    let parse_mode = input.get("parse_mode").and_then(|v| v.as_str());
    let reply_to = input.get("reply_to_message_id").and_then(|v| v.as_i64());

    let response = client
        .send_message(chat_id, text, parse_mode, reply_to)
        .await?;

    Ok(ActionResult {
        success: true,
        output: serde_json::json!({ "message": response }),
        message: format!("sent message to chat {chat_id}"),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::client::test_helpers::MockTelegramApi;

    #[test]
    fn test_declaration_name() {
        assert_eq!(declaration().name, "send_message");
    }

    #[tokio::test]
    async fn test_execute_missing_chat_id() {
        let mock = MockTelegramApi {
            response: serde_json::json!({}),
        };
        let input = serde_json::json!({ "text": "hello" });
        let result = execute(&mock, &input).await;
        assert!(matches!(result, Err(TelegramError::InvalidInput(_))));
    }

    #[tokio::test]
    async fn test_execute_missing_text() {
        let mock = MockTelegramApi {
            response: serde_json::json!({}),
        };
        let input = serde_json::json!({ "chat_id": "123" });
        let result = execute(&mock, &input).await;
        assert!(matches!(result, Err(TelegramError::InvalidInput(_))));
    }

    #[tokio::test]
    async fn test_execute_success() {
        let mock = MockTelegramApi {
            response: serde_json::json!({ "message_id": 42, "text": "hello" }),
        };
        let input = serde_json::json!({ "chat_id": "123", "text": "hello" });
        let result = execute(&mock, &input).await.unwrap();
        assert!(result.success);
    }
}
