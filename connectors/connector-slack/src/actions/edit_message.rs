use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::client::SlackApi;
use crate::error::SlackError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        read_only: false,
        name: "edit_message".to_owned(),
        description: "Edit an existing Slack message.".to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "channel": { "type": "string", "description": "Channel ID" },
                "ts": { "type": "string", "description": "Message timestamp to edit" },
                "text": { "type": "string", "description": "New message text" }
            },
            "required": ["channel", "ts", "text"]
        })),
        output_schema: None,
    }
}

pub async fn execute(
    client: &dyn SlackApi,
    input: &serde_json::Value,
) -> Result<ActionResult, SlackError> {
    let channel = input
        .get("channel")
        .and_then(|v| v.as_str())
        .ok_or_else(|| SlackError::InvalidInput("missing 'channel'".into()))?;

    let ts = input
        .get("ts")
        .and_then(|v| v.as_str())
        .ok_or_else(|| SlackError::InvalidInput("missing 'ts'".into()))?;

    let text = input
        .get("text")
        .and_then(|v| v.as_str())
        .ok_or_else(|| SlackError::InvalidInput("missing 'text'".into()))?;

    client.edit_message(channel, ts, text).await?;

    Ok(ActionResult {
        success: true,
        output: serde_json::json!({}),
        message: format!("edited message {ts} in channel {channel}"),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::client::test_helpers::MockSlackApi;

    #[tokio::test]
    async fn test_execute_edit_message_success() {
        let client = MockSlackApi;
        let input = serde_json::json!({
            "channel": "C123",
            "ts": "1234567890.123456",
            "text": "updated"
        });
        let result = execute(&client, &input).await.unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_execute_edit_message_missing_ts() {
        let client = MockSlackApi;
        let input = serde_json::json!({ "channel": "C123", "text": "updated" });
        assert!(execute(&client, &input).await.is_err());
    }
}
