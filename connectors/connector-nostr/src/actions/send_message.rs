use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::client::NostrApi;
use crate::error::NostrError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        name: "send_message".to_owned(),
        description: "Send a message — routes to send_dm (if chat_id is a pubkey) or publish_note.".to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "chat_id": { "type": "string", "description": "Recipient pubkey (hex/npub) for DM, or empty for public note." },
                "text": { "type": "string", "description": "Message content." }
            },
            "required": ["text"]
        })),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "event_id": { "type": "string" }
            }
        })),
    }
}

/// Routes to send_dm or publish_note based on whether chat_id looks like a pubkey.
pub async fn execute(
    client: &dyn NostrApi,
    input: &serde_json::Value,
) -> Result<ActionResult, NostrError> {
    let text = input
        .get("text")
        .or_else(|| input.get("content"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| NostrError::InvalidInput("missing 'text'".to_owned()))?;

    let chat_id = input
        .get("chat_id")
        .or_else(|| input.get("recipient_pubkey"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // If chat_id looks like a hex pubkey (64 chars) or npub, route to DM
    let is_pubkey = chat_id.len() == 64 && chat_id.chars().all(|c| c.is_ascii_hexdigit())
        || chat_id.starts_with("npub1");

    if is_pubkey && !chat_id.is_empty() {
        let event_id = client.send_dm(chat_id, text).await?;
        Ok(ActionResult {
            success: true,
            output: serde_json::json!({ "event_id": event_id }),
            message: format!("sent encrypted DM to {chat_id}"),
        })
    } else {
        let event_id = client.publish_note(text).await?;
        Ok(ActionResult {
            success: true,
            output: serde_json::json!({ "event_id": event_id }),
            message: format!("published note: {event_id}"),
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::client::test_helpers::MockNostrApi;

    #[test]
    fn test_declaration_name() {
        assert_eq!(declaration().name, "send_message");
    }

    #[tokio::test]
    async fn test_execute_with_pubkey_sends_dm() {
        let mock = MockNostrApi {
            response_id: "dm456".into(),
        };
        let input = serde_json::json!({
            "chat_id": "a".repeat(64), // 64-char hex = pubkey
            "text": "hello via DM"
        });
        let result = execute(&mock, &input).await.unwrap();
        assert!(result.success);
        assert!(result.message.contains("DM"));
    }

    #[tokio::test]
    async fn test_execute_without_pubkey_publishes_note() {
        let mock = MockNostrApi {
            response_id: "note789".into(),
        };
        let input = serde_json::json!({
            "text": "hello world"
        });
        let result = execute(&mock, &input).await.unwrap();
        assert!(result.success);
        assert!(result.message.contains("published"));
    }

    #[tokio::test]
    async fn test_execute_with_npub_sends_dm() {
        let mock = MockNostrApi {
            response_id: "dm_npub".into(),
        };
        let input = serde_json::json!({
            "chat_id": "npub1qqqqqq",
            "text": "hello npub"
        });
        let result = execute(&mock, &input).await.unwrap();
        assert!(result.success);
        assert!(result.message.contains("DM"));
    }

    #[tokio::test]
    async fn test_execute_bot_response_format() {
        // Bot response dispatcher sends {"chat_id": X, "text": Y}
        let mock = MockNostrApi {
            response_id: "bot_resp".into(),
        };
        let input = serde_json::json!({
            "chat_id": "a".repeat(64),
            "text": "bot response"
        });
        let result = execute(&mock, &input).await.unwrap();
        assert!(result.success);
    }
}
