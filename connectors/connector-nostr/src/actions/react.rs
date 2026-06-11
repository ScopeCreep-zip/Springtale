use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::client::NostrApi;
use crate::error::NostrError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        read_only: false,
        name: "react".to_owned(),
        description: "React to a Nostr event with an emoji (kind 7).".to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "event_id": { "type": "string", "description": "Event ID to react to." },
                "reaction": { "type": "string", "description": "Reaction emoji (e.g., '+', '❤️').", "default": "+" }
            },
            "required": ["event_id"]
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
    let reaction = input
        .get("reaction")
        .and_then(|v| v.as_str())
        .unwrap_or("+");

    let result_id = client.react(event_id, reaction).await?;

    Ok(ActionResult {
        success: true,
        output: serde_json::json!({ "event_id": result_id }),
        message: format!("reacted to {event_id} with {reaction}"),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::client::test_helpers::MockNostrApi;

    #[test]
    fn test_declaration_name() {
        assert_eq!(declaration().name, "react");
    }

    #[tokio::test]
    async fn test_execute_success() {
        let mock = MockNostrApi {
            response_id: "react123".into(),
        };
        let input = serde_json::json!({ "event_id": "target123", "reaction": "❤️" });
        let result = execute(&mock, &input).await.unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_execute_default_reaction() {
        let mock = MockNostrApi {
            response_id: "react456".into(),
        };
        let input = serde_json::json!({ "event_id": "target456" });
        let result = execute(&mock, &input).await.unwrap();
        assert!(result.success);
        assert!(result.message.contains("+"));
    }

    #[tokio::test]
    async fn test_execute_missing_event_id() {
        let mock = MockNostrApi {
            response_id: "x".into(),
        };
        let input = serde_json::json!({ "reaction": "+" });
        let result = execute(&mock, &input).await;
        assert!(matches!(result, Err(NostrError::InvalidInput(_))));
    }
}
