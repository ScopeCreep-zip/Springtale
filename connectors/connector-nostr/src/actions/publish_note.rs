use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::client::NostrApi;
use crate::error::NostrError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        read_only: false,
        name: "publish_note".to_owned(),
        description: "Publish a public text note (kind 1) to connected relays.".to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "content": { "type": "string", "description": "Note text content." }
            },
            "required": ["content"]
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
    let content = input
        .get("content")
        .and_then(|v| v.as_str())
        .ok_or_else(|| NostrError::InvalidInput("missing 'content'".to_owned()))?;

    let event_id = client.publish_note(content).await?;

    Ok(ActionResult {
        success: true,
        output: serde_json::json!({ "event_id": event_id }),
        message: format!("published note: {event_id}"),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::client::test_helpers::MockNostrApi;

    #[test]
    fn test_declaration_name() {
        assert_eq!(declaration().name, "publish_note");
    }

    #[tokio::test]
    async fn test_execute_success() {
        let mock = MockNostrApi {
            response_id: "abc123".into(),
        };
        let input = serde_json::json!({ "content": "Hello Nostr!" });
        let result = execute(&mock, &input).await.unwrap();
        assert!(result.success);
        assert_eq!(result.output["event_id"], "abc123");
    }

    #[tokio::test]
    async fn test_execute_missing_content() {
        let mock = MockNostrApi {
            response_id: "x".into(),
        };
        let input = serde_json::json!({});
        let result = execute(&mock, &input).await;
        assert!(matches!(result, Err(NostrError::InvalidInput(_))));
    }
}
