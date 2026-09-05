use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::client::SignalApi;
use crate::error::SignalError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        read_only: false,
        destructive: None,
        name: "send_message".to_owned(),
        description: "Send an E2E encrypted Signal message to one or more recipients.".to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "recipients": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Phone numbers or UUIDs"
                },
                "chat_id": { "type": "string", "description": "Alias for single recipient (bot response routing)" },
                "text": { "type": "string", "description": "Message text" }
            },
            "required": ["text"]
        })),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "timestamp": { "type": "integer" }
            }
        })),
    }
}

pub async fn execute(
    client: &dyn SignalApi,
    input: &serde_json::Value,
) -> Result<ActionResult, SignalError> {
    let text = input
        .get("text")
        .and_then(|v| v.as_str())
        .ok_or_else(|| SignalError::InvalidInput("missing 'text'".into()))?;

    // Accept "recipients" array or "chat_id" for single recipient.
    // D1: each value may also be a `WorkspaceKey` URI
    // (`signal://group/{id}` or `signal://user/{phone}`); parse
    // through the boundary translator which falls back to raw-id
    // semantics when no `://` is present.
    let parse_one = |raw: &str| {
        springtale_connector::workspace_key::extract_id_for_scheme(raw, "connector-signal")
            .map(str::to_owned)
            .map_err(|e| SignalError::InvalidInput(e.to_string()))
    };
    let recipients: Vec<String> =
        if let Some(arr) = input.get("recipients").and_then(|v| v.as_array()) {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(parse_one)
                .collect::<Result<Vec<_>, _>>()?
        } else if let Some(chat_id) = input.get("chat_id").and_then(|v| v.as_str()) {
            vec![parse_one(chat_id)?]
        } else {
            return Err(SignalError::InvalidInput(
                "missing 'recipients' or 'chat_id'".into(),
            ));
        };

    if recipients.is_empty() {
        return Err(SignalError::InvalidInput("no recipients provided".into()));
    }

    let response = client.send_message(&recipients, text).await?;

    Ok(ActionResult {
        success: true,
        output: response,
        message: format!("sent Signal message to {} recipient(s)", recipients.len()),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::client::test_helpers::MockSignalApi;

    #[tokio::test]
    async fn test_send_message_with_recipients() {
        let client = MockSignalApi;
        let input = serde_json::json!({
            "recipients": ["+1234567890"],
            "text": "hello"
        });
        let result = execute(&client, &input).await.unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_send_message_with_chat_id() {
        let client = MockSignalApi;
        let input = serde_json::json!({
            "chat_id": "+1234567890",
            "text": "hello"
        });
        let result = execute(&client, &input).await.unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_send_message_missing_text() {
        let client = MockSignalApi;
        let input = serde_json::json!({ "recipients": ["+1"] });
        assert!(execute(&client, &input).await.is_err());
    }

    #[tokio::test]
    async fn test_send_message_missing_recipients() {
        let client = MockSignalApi;
        let input = serde_json::json!({ "text": "hello" });
        assert!(execute(&client, &input).await.is_err());
    }
}
