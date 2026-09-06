use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::client::SignalApi;
use crate::error::SignalError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        read_only: false,
        destructive: None,
        poll_interval_secs: None,
        name: "set_disappearing_timer".to_owned(),
        description: "Set the disappearing message timer for a 1:1 Signal conversation. \
                      Set to 0 to disable. Note: group disappearing messages have limited \
                      support in signal-cli."
            .to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "recipient": { "type": "string", "description": "Phone number or UUID" },
                "expires_in_seconds": { "type": "integer", "description": "Timer in seconds (0 = disable)" }
            },
            "required": ["recipient", "expires_in_seconds"]
        })),
        output_schema: None,
    }
}

pub async fn execute(
    client: &dyn SignalApi,
    input: &serde_json::Value,
) -> Result<ActionResult, SignalError> {
    let recipient = input
        .get("recipient")
        .and_then(|v| v.as_str())
        .ok_or_else(|| SignalError::InvalidInput("missing 'recipient'".into()))?;

    let expires = input
        .get("expires_in_seconds")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| SignalError::InvalidInput("missing 'expires_in_seconds'".into()))?;

    client.set_disappearing_timer(recipient, expires).await?;

    let msg = if expires == 0 {
        format!("disabled disappearing messages for {recipient}")
    } else {
        format!("set disappearing timer to {expires}s for {recipient}")
    };

    Ok(ActionResult {
        success: true,
        output: serde_json::json!({}),
        message: msg,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::client::test_helpers::MockSignalApi;

    #[tokio::test]
    async fn test_set_timer_success() {
        let client = MockSignalApi;
        let input = serde_json::json!({ "recipient": "+1234567890", "expires_in_seconds": 3600 });
        let result = execute(&client, &input).await.unwrap();
        assert!(result.success);
        assert!(result.message.contains("3600s"));
    }

    #[tokio::test]
    async fn test_disable_timer() {
        let client = MockSignalApi;
        let input = serde_json::json!({ "recipient": "+1234567890", "expires_in_seconds": 0 });
        let result = execute(&client, &input).await.unwrap();
        assert!(result.message.contains("disabled"));
    }
}
