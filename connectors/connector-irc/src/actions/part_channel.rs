use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::client::IrcApi;
use crate::error::IrcError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        name: "part_channel".to_owned(),
        description: "Leave an IRC channel.".to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "channel": { "type": "string" }
            },
            "required": ["channel"]
        })),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": { "parted": { "type": "boolean" } }
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

    client.part_channel(channel).await?;

    Ok(ActionResult {
        success: true,
        output: serde_json::json!({ "parted": true }),
        message: format!("left {channel}"),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::client::test_helpers::MockIrcApi;

    #[test]
    fn test_declaration_name() {
        assert_eq!(declaration().name, "part_channel");
    }

    #[tokio::test]
    async fn test_execute_success() {
        let mock = MockIrcApi;
        let input = serde_json::json!({ "channel": "#general" });
        assert!(execute(&mock, &input).await.unwrap().success);
    }
}
