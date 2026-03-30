use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::client::KickApi;
use crate::error::KickError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        name: "get_channel".to_owned(),
        description: "Get information about a Kick channel by slug.".to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "slug": {
                    "type": "string",
                    "description": "Channel slug (e.g., 'xqc'). Used as query param to GET /public/v1/channels."
                }
            },
            "required": ["slug"]
        })),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "channel": { "type": "object", "description": "Channel information from Kick API." }
            }
        })),
    }
}

pub async fn execute(
    client: &dyn KickApi,
    input: &serde_json::Value,
) -> Result<ActionResult, KickError> {
    let slug = input.get("slug").and_then(|v| v.as_str())
        .ok_or_else(|| KickError::InvalidInput("missing 'slug'".to_owned()))?;

    let channel = client.get_channel_by_slug(slug).await?;

    Ok(ActionResult {
        success: true,
        output: serde_json::json!({ "channel": channel }),
        message: format!("fetched channel {slug}"),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::client::KickClient;

    fn test_client() -> KickClient {
        KickClient::new("http://localhost:0", secrecy::SecretBox::new(Box::new("fake_token".to_owned()))).unwrap()
    }

    #[test]
    fn test_declaration() {
        let decl = declaration();
        assert_eq!(decl.name, "get_channel");
    }

    #[tokio::test]
    async fn test_execute_missing_slug_returns_invalid_input() {
        let client = test_client();
        let input = serde_json::json!({});
        let result = execute(&client, &input).await;
        assert!(matches!(result, Err(KickError::InvalidInput(_))));
    }

    #[tokio::test]
    async fn test_execute_null_slug_returns_invalid_input() {
        let client = test_client();
        let input = serde_json::json!({ "slug": null });
        let result = execute(&client, &input).await;
        assert!(matches!(result, Err(KickError::InvalidInput(_))));
    }

    #[tokio::test]
    async fn test_execute_numeric_slug_returns_invalid_input() {
        let client = test_client();
        let input = serde_json::json!({ "slug": 123 });
        let result = execute(&client, &input).await;
        assert!(matches!(result, Err(KickError::InvalidInput(_))));
    }

    struct MockKickApi;

    #[async_trait::async_trait]
    impl KickApi for MockKickApi {
        async fn send_chat(&self, _channel_id: &str, _message: &str) -> Result<serde_json::Value, KickError> {
            unreachable!()
        }
        async fn get_channel_by_slug(&self, _slug: &str) -> Result<serde_json::Value, KickError> {
            Ok(serde_json::json!({
                "data": [{
                    "id": 12345,
                    "slug": "xqc",
                    "title": "xQc's Channel",
                    "broadcaster_user_id": 67890,
                    "banner_picture": null
                }]
            }))
        }
        async fn get_stream(&self, _channel_id: &str) -> Result<serde_json::Value, KickError> {
            unreachable!()
        }
    }

    #[tokio::test]
    async fn test_execute_get_channel_mock_extracts_response() {
        let mock = MockKickApi;
        let input = serde_json::json!({ "slug": "xqc" });
        let result = execute(&mock, &input).await.unwrap();
        assert!(result.success);
        assert_eq!(result.output["channel"]["data"][0]["slug"], "xqc");
        assert_eq!(result.output["channel"]["data"][0]["title"], "xQc's Channel");
        assert!(result.message.contains("xqc"));
    }
}
