use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::client::BlueskyApi;
use crate::error::BlueskyError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        name: "create_post".to_owned(),
        description: "Create a new post on Bluesky.".to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "text": { "type": "string", "description": "Post text (max 300 characters)." }
            },
            "required": ["text"]
        })),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "uri": { "type": "string" },
                "cid": { "type": "string" },
                "response": { "type": "object" }
            }
        })),
    }
}

pub async fn execute(
    client: &dyn BlueskyApi,
    input: &serde_json::Value,
) -> Result<ActionResult, BlueskyError> {
    let text = input
        .get("text")
        .and_then(|v| v.as_str())
        .ok_or_else(|| BlueskyError::InvalidInput("missing 'text'".to_owned()))?;

    let response = client.create_post(text).await?;

    let uri = response
        .get("uri")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let cid = response
        .get("cid")
        .and_then(|v| v.as_str())
        .unwrap_or_default();

    Ok(ActionResult {
        success: true,
        output: serde_json::json!({
            "uri": uri,
            "cid": cid,
            "response": response,
        }),
        message: format!("created post: {uri}"),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    use crate::client::test_helpers::MockBlueskyClient;

    #[test]
    fn test_declaration_name() {
        let decl = declaration();
        assert_eq!(decl.name, "create_post");
    }

    #[test]
    fn test_declaration_input_schema_required_fields() {
        let decl = declaration();
        let schema = decl
            .input_schema
            .as_ref()
            .unwrap_or_else(|| panic!("input_schema is None"));
        let required = schema
            .get("required")
            .unwrap_or_else(|| panic!("missing required"));
        let required_arr = required
            .as_array()
            .unwrap_or_else(|| panic!("required not array"));
        let required_strs: Vec<&str> = required_arr.iter().filter_map(|v| v.as_str()).collect();
        assert_eq!(required_strs, vec!["text"]);
    }

    #[test]
    fn test_declaration_input_schema_properties() {
        let decl = declaration();
        let schema = decl
            .input_schema
            .as_ref()
            .unwrap_or_else(|| panic!("input_schema is None"));
        let props = schema
            .get("properties")
            .unwrap_or_else(|| panic!("missing properties"));
        let props_obj = props
            .as_object()
            .unwrap_or_else(|| panic!("properties not object"));
        assert!(props_obj.contains_key("text"), "missing 'text' property");
        assert_eq!(props_obj.len(), 1, "expected exactly 1 property");
    }

    #[test]
    fn test_declaration_output_schema_fields() {
        let decl = declaration();
        let schema = decl
            .output_schema
            .as_ref()
            .unwrap_or_else(|| panic!("output_schema is None"));
        let props = schema
            .get("properties")
            .unwrap_or_else(|| panic!("missing properties"));
        let props_obj = props
            .as_object()
            .unwrap_or_else(|| panic!("properties not object"));
        assert!(props_obj.contains_key("uri"), "missing 'uri' output field");
        assert!(props_obj.contains_key("cid"), "missing 'cid' output field");
        assert!(
            props_obj.contains_key("response"),
            "missing 'response' output field"
        );
        assert_eq!(props_obj.len(), 3, "expected exactly 3 output properties");
    }

    #[tokio::test]
    async fn test_execute_missing_text_returns_invalid_input() {
        let mock = MockBlueskyClient {
            response: serde_json::json!({}),
        };
        let input = serde_json::json!({});
        let result = execute(&mock, &input).await;
        assert!(matches!(result.unwrap_err(), BlueskyError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn test_execute_extracts_uri_and_cid_from_response() {
        let mock = MockBlueskyClient {
            response: serde_json::json!({
                "uri": "at://did:plc:abc123/app.bsky.feed.post/3k2la",
                "cid": "bafyreig2"
            }),
        };

        let input = serde_json::json!({ "text": "hello world" });
        let result = execute(&mock, &input).await.unwrap();

        assert!(result.success);
        assert_eq!(
            result.output["uri"],
            "at://did:plc:abc123/app.bsky.feed.post/3k2la"
        );
        assert_eq!(result.output["cid"], "bafyreig2");
        assert!(result.message.contains("at://did:plc:abc123"));
    }

    #[tokio::test]
    async fn test_execute_handles_missing_fields_in_response() {
        let mock = MockBlueskyClient {
            response: serde_json::json!({ "validationStatus": "valid" }),
        };

        let input = serde_json::json!({ "text": "test" });
        let result = execute(&mock, &input).await.unwrap();

        assert!(result.success);
        assert_eq!(result.output["uri"], "");
        assert_eq!(result.output["cid"], "");
    }
}
