use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::client::SlackApi;
use crate::error::SlackError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        read_only: false,
        destructive: None,
        name: "add_reaction".to_owned(),
        description: "Add a reaction emoji to a Slack message.".to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "channel": { "type": "string", "description": "Channel containing the message" },
                "timestamp": { "type": "string", "description": "Message timestamp to react to" },
                "name": { "type": "string", "description": "Emoji name without colons (e.g., thumbsup)" }
            },
            "required": ["channel", "timestamp", "name"]
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

    let timestamp = input
        .get("timestamp")
        .and_then(|v| v.as_str())
        .ok_or_else(|| SlackError::InvalidInput("missing 'timestamp'".into()))?;

    let name = input
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| SlackError::InvalidInput("missing 'name'".into()))?;

    client.add_reaction(channel, timestamp, name).await?;

    Ok(ActionResult {
        success: true,
        output: serde_json::json!({}),
        message: format!("added :{name}: to message in {channel}"),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::client::test_helpers::MockSlackApi;

    #[tokio::test]
    async fn test_execute_add_reaction_success() {
        let client = MockSlackApi;
        let input = serde_json::json!({
            "channel": "C123",
            "timestamp": "1234567890.123456",
            "name": "thumbsup"
        });
        let result = execute(&client, &input).await.unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_execute_add_reaction_missing_name() {
        let client = MockSlackApi;
        let input = serde_json::json!({ "channel": "C123", "timestamp": "123" });
        assert!(execute(&client, &input).await.is_err());
    }
}
