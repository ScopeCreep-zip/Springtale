use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::client::TelegramApi;
use crate::error::TelegramError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        read_only: false,
        name: "send_inline_keyboard".to_owned(),
        description: "Send a message with an inline keyboard.".to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "chat_id": { "type": "string" },
                "text": { "type": "string" },
                "inline_keyboard": {
                    "type": "array",
                    "description": "Array of button rows.",
                    "items": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "text": { "type": "string" },
                                "callback_data": { "type": "string" },
                                "url": { "type": "string" }
                            },
                            "required": ["text"]
                        }
                    }
                }
            },
            "required": ["chat_id", "text", "inline_keyboard"]
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
    let text = input
        .get("text")
        .and_then(|v| v.as_str())
        .ok_or_else(|| TelegramError::InvalidInput("missing 'text'".to_owned()))?;
    let keyboard = input
        .get("inline_keyboard")
        .cloned()
        .ok_or_else(|| TelegramError::InvalidInput("missing 'inline_keyboard'".to_owned()))?;

    let response = client.send_inline_keyboard(chat_id, text, keyboard).await?;

    Ok(ActionResult {
        success: true,
        output: serde_json::json!({ "message": response }),
        message: format!("sent inline keyboard to chat {chat_id}"),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::client::test_helpers::MockTelegramApi;

    #[test]
    fn test_declaration_name() {
        assert_eq!(declaration().name, "send_inline_keyboard");
    }

    #[tokio::test]
    async fn test_execute_missing_keyboard() {
        let mock = MockTelegramApi {
            response: serde_json::json!({}),
        };
        let input = serde_json::json!({ "chat_id": "123", "text": "choose" });
        assert!(matches!(
            execute(&mock, &input).await,
            Err(TelegramError::InvalidInput(_))
        ));
    }

    #[tokio::test]
    async fn test_execute_success() {
        let mock = MockTelegramApi {
            response: serde_json::json!({ "message_id": 1 }),
        };
        let input = serde_json::json!({
            "chat_id": "123",
            "text": "Choose:",
            "inline_keyboard": [[{"text": "Option 1", "callback_data": "opt1"}]]
        });
        assert!(execute(&mock, &input).await.unwrap().success);
    }
}
