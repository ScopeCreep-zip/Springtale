use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::client::TelegramApi;
use crate::error::TelegramError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        name: "edit_message".to_owned(),
        description: "Edit the text of a previously sent message.".to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "chat_id": { "type": "string" },
                "message_id": { "type": "integer", "description": "ID of the message to edit." },
                "text": { "type": "string" },
                "parse_mode": { "type": "string", "enum": ["HTML", "Markdown", "MarkdownV2"] }
            },
            "required": ["chat_id", "message_id", "text"]
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
    let chat_id = input
        .get("chat_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| TelegramError::InvalidInput("missing 'chat_id'".to_owned()))?;
    let message_id = input
        .get("message_id")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| TelegramError::InvalidInput("missing 'message_id'".to_owned()))?;
    let text = input
        .get("text")
        .and_then(|v| v.as_str())
        .ok_or_else(|| TelegramError::InvalidInput("missing 'text'".to_owned()))?;
    let parse_mode = input.get("parse_mode").and_then(|v| v.as_str());

    let response = client
        .edit_message_text(chat_id, message_id, text, parse_mode)
        .await?;

    Ok(ActionResult {
        success: true,
        output: serde_json::json!({ "message": response }),
        message: format!("edited message {message_id} in chat {chat_id}"),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::client::test_helpers::MockTelegramApi;

    #[test]
    fn test_declaration_name() {
        assert_eq!(declaration().name, "edit_message");
    }

    #[tokio::test]
    async fn test_execute_missing_message_id() {
        let mock = MockTelegramApi {
            response: serde_json::json!({}),
        };
        let input = serde_json::json!({ "chat_id": "123", "text": "new text" });
        assert!(matches!(
            execute(&mock, &input).await,
            Err(TelegramError::InvalidInput(_))
        ));
    }

    #[tokio::test]
    async fn test_execute_success() {
        let mock = MockTelegramApi {
            response: serde_json::json!({ "message_id": 42 }),
        };
        let input = serde_json::json!({ "chat_id": "123", "message_id": 42, "text": "edited" });
        assert!(execute(&mock, &input).await.unwrap().success);
    }
}
