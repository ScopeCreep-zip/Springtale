use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::client::KickApi;
use crate::error::KickError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        read_only: true,
        destructive: None,
        name: "get_stream".to_owned(),
        description: "Get the livestream status of a Kick channel.".to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "channel_id": { "type": "string", "description": "Channel ID." }
            },
            "required": ["channel_id"]
        })),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "stream": { "type": "object", "description": "Livestream information." }
            }
        })),
    }
}

pub async fn execute(
    client: &dyn KickApi,
    input: &serde_json::Value,
) -> Result<ActionResult, KickError> {
    let channel_id = input
        .get("channel_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| KickError::InvalidInput("missing 'channel_id'".to_owned()))?;

    let stream = client.get_stream(channel_id).await?;

    Ok(ActionResult {
        success: true,
        output: serde_json::json!({ "stream": stream }),
        message: format!("fetched stream status for channel {channel_id}"),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::client::KickClient;

    fn test_client() -> KickClient {
        KickClient::new(
            "http://localhost:0",
            secrecy::SecretBox::new(Box::new("fake_token".to_owned())),
        )
        .unwrap()
    }

    #[test]
    fn test_declaration() {
        let decl = declaration();
        assert_eq!(decl.name, "get_stream");
    }

    #[tokio::test]
    async fn test_execute_missing_channel_id_returns_invalid_input() {
        let client = test_client();
        let input = serde_json::json!({});
        let result = execute(&client, &input).await;
        assert!(matches!(result, Err(KickError::InvalidInput(_))));
    }

    #[tokio::test]
    async fn test_execute_null_channel_id_returns_invalid_input() {
        let client = test_client();
        let input = serde_json::json!({ "channel_id": null });
        let result = execute(&client, &input).await;
        assert!(matches!(result, Err(KickError::InvalidInput(_))));
    }

    #[tokio::test]
    async fn test_execute_numeric_channel_id_returns_invalid_input() {
        let client = test_client();
        let input = serde_json::json!({ "channel_id": 123 });
        let result = execute(&client, &input).await;
        assert!(matches!(result, Err(KickError::InvalidInput(_))));
    }

    use crate::client::test_helpers::MockKickApi;

    #[tokio::test]
    async fn test_execute_get_stream_mock_extracts_response() {
        let mock = MockKickApi {
            response: serde_json::json!({
                "data": [{
                    "id": 99001,
                    "channel_id": 42,
                    "title": "Late Night Gaming",
                    "is_live": true,
                    "started_at": "2026-03-29T02:00:00Z",
                    "viewer_count": 15432
                }]
            }),
        };
        let input = serde_json::json!({ "channel_id": "42" });
        let result = execute(&mock, &input).await.unwrap();
        assert!(result.success);
        assert_eq!(result.output["stream"]["data"][0]["is_live"], true);
        assert_eq!(
            result.output["stream"]["data"][0]["title"],
            "Late Night Gaming"
        );
        assert_eq!(
            result.output["stream"]["data"][0]["started_at"],
            "2026-03-29T02:00:00Z"
        );
        assert!(result.message.contains("42"));
    }
}
