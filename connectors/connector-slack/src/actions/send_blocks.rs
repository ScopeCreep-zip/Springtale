use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::client::SlackApi;
use crate::error::SlackError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        name: "send_blocks".to_owned(),
        description: "Send a Block Kit message to a Slack channel.".to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "channel": { "type": "string", "description": "Channel ID (C...)" },
                "blocks": {
                    "type": "array",
                    "description": "Block Kit blocks array",
                    "items": { "type": "object" }
                }
            },
            "required": ["channel", "blocks"]
        })),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "ts": { "type": "string" }
            }
        })),
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

    let blocks = input
        .get("blocks")
        .ok_or_else(|| SlackError::InvalidInput("missing 'blocks'".into()))?
        .clone();

    let response = client.send_blocks(channel, blocks).await?;

    let ts = response.get("ts").and_then(|t| t.as_str()).unwrap_or("");

    Ok(ActionResult {
        success: true,
        output: serde_json::json!({ "ts": ts }),
        message: format!("sent blocks to channel {channel}"),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::client::test_helpers::MockSlackApi;

    #[tokio::test]
    async fn test_execute_send_blocks_success() {
        let client = MockSlackApi;
        let input = serde_json::json!({
            "channel": "C123",
            "blocks": [{ "type": "section", "text": { "type": "mrkdwn", "text": "Hello" } }]
        });
        let result = execute(&client, &input).await.unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_execute_send_blocks_missing_blocks() {
        let client = MockSlackApi;
        let input = serde_json::json!({ "channel": "C123" });
        assert!(execute(&client, &input).await.is_err());
    }
}
