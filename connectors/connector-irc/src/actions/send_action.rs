use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::client::IrcApi;
use crate::error::IrcError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        name: "send_action".to_owned(),
        description: "Send a /me action to a channel.".to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "target": { "type": "string" },
                "action": { "type": "string", "description": "Action text (e.g., 'waves hello')." }
            },
            "required": ["target", "action"]
        })),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": { "sent": { "type": "boolean" } }
        })),
    }
}

pub async fn execute(
    client: &dyn IrcApi,
    input: &serde_json::Value,
) -> Result<ActionResult, IrcError> {
    let target = input
        .get("target")
        .and_then(|v| v.as_str())
        .ok_or_else(|| IrcError::InvalidInput("missing 'target'".into()))?;
    let action = input
        .get("action")
        .and_then(|v| v.as_str())
        .ok_or_else(|| IrcError::InvalidInput("missing 'action'".into()))?;

    client.send_action(target, action).await?;

    Ok(ActionResult {
        success: true,
        output: serde_json::json!({ "sent": true }),
        message: format!("sent action to {target}"),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::client::test_helpers::MockIrcApi;

    #[test]
    fn test_declaration_name() {
        assert_eq!(declaration().name, "send_action");
    }

    #[tokio::test]
    async fn test_execute_success() {
        let mock = MockIrcApi;
        let input = serde_json::json!({ "target": "#general", "action": "waves" });
        assert!(execute(&mock, &input).await.unwrap().success);
    }
}
