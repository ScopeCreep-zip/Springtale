use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::client::SlackApi;
use crate::error::SlackError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        name: "send_thread_reply".to_owned(),
        description: "Send a reply in a Slack thread.".to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "channel": { "type": "string", "description": "Channel ID" },
                "thread_ts": { "type": "string", "description": "Parent message timestamp" },
                "text": { "type": "string", "description": "Reply text" },
                "reply_broadcast": { "type": "boolean", "description": "Also post to channel (default: false)" }
            },
            "required": ["channel", "thread_ts", "text"]
        })),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": { "ts": { "type": "string" } }
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

    let thread_ts = input
        .get("thread_ts")
        .and_then(|v| v.as_str())
        .ok_or_else(|| SlackError::InvalidInput("missing 'thread_ts'".into()))?;

    let text = input
        .get("text")
        .and_then(|v| v.as_str())
        .ok_or_else(|| SlackError::InvalidInput("missing 'text'".into()))?;

    let broadcast = input
        .get("reply_broadcast")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let response = client
        .send_thread_reply(channel, thread_ts, text, broadcast)
        .await?;

    let ts = response.get("ts").and_then(|t| t.as_str()).unwrap_or("");

    Ok(ActionResult {
        success: true,
        output: serde_json::json!({ "ts": ts }),
        message: format!("sent thread reply in channel {channel}"),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::client::test_helpers::MockSlackApi;

    #[tokio::test]
    async fn test_execute_thread_reply_success() {
        let client = MockSlackApi;
        let input = serde_json::json!({
            "channel": "C123",
            "thread_ts": "1234567890.123456",
            "text": "reply"
        });
        let result = execute(&client, &input).await.unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_execute_thread_reply_missing_thread_ts() {
        let client = MockSlackApi;
        let input = serde_json::json!({ "channel": "C123", "text": "reply" });
        assert!(execute(&client, &input).await.is_err());
    }
}
