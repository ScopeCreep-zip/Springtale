use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::client::NostrApi;
use crate::error::NostrError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        read_only: false,
        name: "reply".to_owned(),
        description: "Reply to a Nostr note (kind 1 with e/p tags).".to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "event_id": { "type": "string", "description": "Event ID to reply to." },
                "content": { "type": "string", "description": "Reply text content." }
            },
            "required": ["event_id", "content"]
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
    let event_id = input
        .get("event_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| NostrError::InvalidInput("missing 'event_id'".to_owned()))?;
    let content = input
        .get("content")
        .and_then(|v| v.as_str())
        .ok_or_else(|| NostrError::InvalidInput("missing 'content'".to_owned()))?;

    let result_id = client.reply(event_id, content).await?;

    Ok(ActionResult {
        success: true,
        output: serde_json::json!({ "event_id": result_id }),
        message: format!("replied to {event_id}"),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::client::test_helpers::MockNostrApi;

    #[test]
    fn test_declaration_name() {
        assert_eq!(declaration().name, "reply");
    }

    #[tokio::test]
    async fn test_execute_success() {
        let mock = MockNostrApi {
            response_id: "reply123".into(),
        };
        let input = serde_json::json!({ "event_id": "target789", "content": "Great post!" });
        let result = execute(&mock, &input).await.unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_execute_missing_event_id() {
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
        let input = serde_json::json!({ "event_id": "abc" });
        let result = execute(&mock, &input).await;
        assert!(matches!(result, Err(NostrError::InvalidInput(_))));
    }
}
