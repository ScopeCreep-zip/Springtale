use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::client::SignalApi;
use crate::error::SignalError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        read_only: false,
        destructive: None,
        name: "send_group_message".to_owned(),
        description: "Send a message to a Signal group.".to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "group_id": { "type": "string", "description": "Base64-encoded group ID" },
                "text": { "type": "string", "description": "Message text" }
            },
            "required": ["group_id", "text"]
        })),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": { "timestamp": { "type": "integer" } }
        })),
    }
}

pub async fn execute(
    client: &dyn SignalApi,
    input: &serde_json::Value,
) -> Result<ActionResult, SignalError> {
    let group_id = input
        .get("group_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| SignalError::InvalidInput("missing 'group_id'".into()))?;

    let text = input
        .get("text")
        .and_then(|v| v.as_str())
        .ok_or_else(|| SignalError::InvalidInput("missing 'text'".into()))?;

    let response = client.send_group_message(group_id, text).await?;

    Ok(ActionResult {
        success: true,
        output: response,
        message: format!("sent group message to {group_id}"),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::client::test_helpers::MockSignalApi;

    #[tokio::test]
    async fn test_send_group_message_success() {
        let client = MockSignalApi;
        let input = serde_json::json!({ "group_id": "abc123", "text": "hello group" });
        let result = execute(&client, &input).await.unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_send_group_message_missing_group_id() {
        let client = MockSignalApi;
        let input = serde_json::json!({ "text": "hello" });
        assert!(execute(&client, &input).await.is_err());
    }
}
