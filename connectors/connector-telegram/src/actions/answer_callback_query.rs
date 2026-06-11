use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::client::TelegramApi;
use crate::error::TelegramError;

/// Declare the `answer_callback_query` action for connector-telegram.
///
/// Telegram requires bots to acknowledge callback_query updates within 10
/// seconds of receipt, otherwise the button shows a perpetual loading spinner.
/// Use this action in any rule that handles a `callback_query_received` trigger.
pub fn declaration() -> ActionDecl {
    ActionDecl {
        read_only: false,
        name: "answer_callback_query".to_owned(),
        description:
            "Acknowledge an inline keyboard callback_query. Must be called within 10 seconds."
                .to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "callback_query_id": {
                    "type": "string",
                    "description": "Callback query ID from the callback_query_received trigger."
                },
                "text": {
                    "type": "string",
                    "description": "Optional notification text (0-200 chars). Omit to dismiss silently."
                },
                "show_alert": {
                    "type": "boolean",
                    "description": "If true, show a modal popup instead of a toast notification.",
                    "default": false
                }
            },
            "required": ["callback_query_id"]
        })),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "ok": { "type": "boolean" }
            }
        })),
    }
}

pub async fn execute(
    client: &dyn TelegramApi,
    input: &serde_json::Value,
) -> Result<ActionResult, TelegramError> {
    let callback_query_id = input
        .get("callback_query_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| TelegramError::InvalidInput("missing 'callback_query_id'".to_owned()))?;
    let text = input.get("text").and_then(|v| v.as_str());
    let show_alert = input
        .get("show_alert")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let response = client
        .answer_callback_query(callback_query_id, text, show_alert)
        .await?;

    Ok(ActionResult {
        success: true,
        output: serde_json::json!({ "ok": true, "response": response }),
        message: format!("answered callback_query {callback_query_id}"),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::client::test_helpers::MockTelegramApi;

    #[test]
    fn test_declaration_name() {
        assert_eq!(declaration().name, "answer_callback_query");
    }

    #[tokio::test]
    async fn test_execute_missing_id() {
        let mock = MockTelegramApi {
            response: serde_json::json!({ "ok": true }),
        };
        let input = serde_json::json!({ "text": "hi" });
        assert!(matches!(
            execute(&mock, &input).await,
            Err(TelegramError::InvalidInput(_))
        ));
    }

    #[tokio::test]
    async fn test_execute_success_silent() {
        let mock = MockTelegramApi {
            response: serde_json::json!({ "ok": true }),
        };
        let input = serde_json::json!({ "callback_query_id": "cb_123" });
        let result = execute(&mock, &input).await.unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_execute_success_with_alert() {
        let mock = MockTelegramApi {
            response: serde_json::json!({ "ok": true }),
        };
        let input = serde_json::json!({
            "callback_query_id": "cb_456",
            "text": "Done!",
            "show_alert": true
        });
        let result = execute(&mock, &input).await.unwrap();
        assert!(result.success);
    }
}
