use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::client::TelegramApi;
use crate::error::TelegramError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        name: "send_photo".to_owned(),
        description: "Send a photo to a Telegram chat.".to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "chat_id": { "type": "string" },
                "photo": { "type": "string", "description": "File ID, URL, or file path." },
                "caption": { "type": "string", "description": "Optional photo caption." }
            },
            "required": ["chat_id", "photo"]
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
    let photo = input
        .get("photo")
        .and_then(|v| v.as_str())
        .ok_or_else(|| TelegramError::InvalidInput("missing 'photo'".to_owned()))?;
    let caption = input.get("caption").and_then(|v| v.as_str());

    let response = client.send_photo(chat_id, photo, caption).await?;

    Ok(ActionResult {
        success: true,
        output: serde_json::json!({ "message": response }),
        message: format!("sent photo to chat {chat_id}"),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::client::test_helpers::MockTelegramApi;

    #[test]
    fn test_declaration_name() {
        assert_eq!(declaration().name, "send_photo");
    }

    #[tokio::test]
    async fn test_execute_missing_photo() {
        let mock = MockTelegramApi {
            response: serde_json::json!({}),
        };
        let input = serde_json::json!({ "chat_id": "123" });
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
        let input = serde_json::json!({ "chat_id": "123", "photo": "https://example.com/img.jpg" });
        assert!(execute(&mock, &input).await.unwrap().success);
    }
}
