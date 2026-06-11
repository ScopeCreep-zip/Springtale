use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::client::IrcApi;
use crate::error::IrcError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        read_only: false,
        name: "join_channel".to_owned(),
        description: "Join an IRC channel.".to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "channel": { "type": "string", "description": "Channel name (e.g., #general)." }
            },
            "required": ["channel"]
        })),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": { "joined": { "type": "boolean" } }
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

    client.join_channel(channel).await?;

    Ok(ActionResult {
        success: true,
        output: serde_json::json!({ "joined": true }),
        message: format!("joined {channel}"),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::client::test_helpers::MockIrcApi;

    #[test]
    fn test_declaration_name() {
        assert_eq!(declaration().name, "join_channel");
    }

    #[tokio::test]
    async fn test_execute_success() {
        let mock = MockIrcApi;
        let input = serde_json::json!({ "channel": "#general" });
        assert!(execute(&mock, &input).await.unwrap().success);
    }

    #[tokio::test]
    async fn test_execute_missing_channel() {
        let mock = MockIrcApi;
        assert!(execute(&mock, &serde_json::json!({})).await.is_err());
    }
}
