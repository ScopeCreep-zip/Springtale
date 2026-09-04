use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::client::IrcApi;
use crate::error::IrcError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        read_only: false,
        destructive: None,
        name: "send_message".to_owned(),
        description: "Send a message to a channel or user via PRIVMSG.".to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "target": { "type": "string", "description": "Channel (#name) or nick." },
                "message": { "type": "string" }
            },
            "required": ["target", "message"]
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
    let raw_target = input
        .get("target")
        .or_else(|| input.get("chat_id")) // bot response compat
        .and_then(|v| v.as_str())
        .ok_or_else(|| IrcError::InvalidInput("missing 'target'".into()))?;
    // D1 — accept raw `#channel` / nick OR an `irc://network/.../channel/#name`
    // URI. The parser's last-segment extraction returns the
    // channel name or nick.
    let target =
        springtale_connector::workspace_key::extract_id_for_scheme(raw_target, "connector-irc")
            .map_err(|e| IrcError::InvalidInput(e.to_string()))?;
    let message = input
        .get("message")
        .or_else(|| input.get("text")) // bot response compat
        .and_then(|v| v.as_str())
        .ok_or_else(|| IrcError::InvalidInput("missing 'message'".into()))?;

    client.send_message(target, message).await?;

    Ok(ActionResult {
        success: true,
        output: serde_json::json!({ "sent": true }),
        message: format!("sent message to {target}"),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::client::test_helpers::MockIrcApi;

    #[test]
    fn test_declaration_name() {
        assert_eq!(declaration().name, "send_message");
    }

    #[tokio::test]
    async fn test_execute_success() {
        let mock = MockIrcApi;
        let input = serde_json::json!({ "target": "#general", "message": "hello" });
        let result = execute(&mock, &input).await.unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_execute_missing_target() {
        let mock = MockIrcApi;
        let input = serde_json::json!({ "message": "hello" });
        assert!(matches!(
            execute(&mock, &input).await,
            Err(IrcError::InvalidInput(_))
        ));
    }

    #[tokio::test]
    async fn test_execute_missing_message() {
        let mock = MockIrcApi;
        let input = serde_json::json!({ "target": "#chan" });
        assert!(matches!(
            execute(&mock, &input).await,
            Err(IrcError::InvalidInput(_))
        ));
    }
}
