use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::client::NostrApi;
use crate::error::NostrError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        read_only: false,
        destructive: None,
        poll_interval_secs: None,
        name: "send_dm".to_owned(),
        description: "Send an encrypted DM via NIP-44 (gift-wrapped via NIP-17).".to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "recipient_pubkey": { "type": "string", "description": "Recipient's hex public key or npub." },
                "content": { "type": "string", "description": "Message content (will be encrypted)." }
            },
            "required": ["recipient_pubkey", "content"]
        })),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "event_id": { "type": "string" }
            }
        })),
    }
}

pub async fn execute(
    client: &dyn NostrApi,
    input: &serde_json::Value,
) -> Result<ActionResult, NostrError> {
    let recipient = input
        .get("recipient_pubkey")
        .and_then(|v| v.as_str())
        .ok_or_else(|| NostrError::InvalidInput("missing 'recipient_pubkey'".to_owned()))?;
    let content = input
        .get("content")
        .and_then(|v| v.as_str())
        .ok_or_else(|| NostrError::InvalidInput("missing 'content'".to_owned()))?;

    let event_id = client.send_dm(recipient, content).await?;

    Ok(ActionResult {
        success: true,
        output: serde_json::json!({ "event_id": event_id }),
        message: format!("sent encrypted DM to {recipient}"),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::client::test_helpers::MockNostrApi;

    #[test]
    fn test_declaration_name() {
        assert_eq!(declaration().name, "send_dm");
    }

    #[tokio::test]
    async fn test_execute_success() {
        let mock = MockNostrApi {
            response_id: "dm123".into(),
        };
        let input = serde_json::json!({
            "recipient_pubkey": "npub1abc",
            "content": "secret message"
        });
        let result = execute(&mock, &input).await.unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_execute_missing_recipient() {
        let mock = MockNostrApi {
            response_id: "x".into(),
        };
        let input = serde_json::json!({ "content": "hello" });
        let result = execute(&mock, &input).await;
        assert!(matches!(result, Err(NostrError::InvalidInput(_))));
    }

    #[tokio::test]
    async fn test_execute_missing_content() {
        let mock = MockNostrApi {
            response_id: "x".into(),
        };
        let input = serde_json::json!({ "recipient_pubkey": "npub1abc" });
        let result = execute(&mock, &input).await;
        assert!(matches!(result, Err(NostrError::InvalidInput(_))));
    }
}
