use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::client::IrcApi;
use crate::error::IrcError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        read_only: false,
        name: "set_topic".to_owned(),
        description: "Set or change a channel's topic.".to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "channel": { "type": "string" },
                "topic": { "type": "string" }
            },
            "required": ["channel", "topic"]
        })),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": { "success": { "type": "boolean" } }
        })),
    }
}

pub async fn execute(
    client: &dyn IrcApi,
    input: &serde_json::Value,
) -> Result<ActionResult, IrcError> {
    let channel = input
        .get("channel")
        .and_then(|v| v.as_str())
        .ok_or_else(|| IrcError::InvalidInput("missing 'channel'".into()))?;
    let topic = input
        .get("topic")
        .and_then(|v| v.as_str())
        .ok_or_else(|| IrcError::InvalidInput("missing 'topic'".into()))?;

    client.set_topic(channel, topic).await?;

    Ok(ActionResult {
        success: true,
        output: serde_json::json!({ "success": true }),
        message: format!("set topic in {channel}"),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::client::test_helpers::MockIrcApi;

    #[test]
    fn test_declaration_name() {
        assert_eq!(declaration().name, "set_topic");
    }

    #[tokio::test]
    async fn test_execute_success() {
        let mock = MockIrcApi;
        let input = serde_json::json!({ "channel": "#general", "topic": "Welcome!" });
        assert!(execute(&mock, &input).await.unwrap().success);
    }
}
