use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::client::BlueskyApi;
use crate::error::BlueskyError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        name: "repost".to_owned(),
        description: "Repost a Bluesky post.".to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "subject_uri": { "type": "string", "description": "AT URI of the post to repost." },
                "subject_cid": { "type": "string", "description": "CID of the post to repost." }
            },
            "required": ["subject_uri", "subject_cid"]
        })),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "uri": { "type": "string" },
                "response": { "type": "object" }
            }
        })),
    }
}

pub async fn execute(
    client: &dyn BlueskyApi,
    input: &serde_json::Value,
) -> Result<ActionResult, BlueskyError> {
    let subject_uri = input
        .get("subject_uri")
        .and_then(|v| v.as_str())
        .ok_or_else(|| BlueskyError::InvalidInput("missing 'subject_uri'".to_owned()))?;
    let subject_cid = input
        .get("subject_cid")
        .and_then(|v| v.as_str())
        .ok_or_else(|| BlueskyError::InvalidInput("missing 'subject_cid'".to_owned()))?;

    let response = client.repost(subject_uri, subject_cid).await?;

    let uri = response
        .get("uri")
        .and_then(|v| v.as_str())
        .unwrap_or_default();

    Ok(ActionResult {
        success: true,
        output: serde_json::json!({
            "uri": uri,
            "response": response,
        }),
        message: format!("reposted {subject_uri}"),
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
        assert_eq!(decl.name, "repost");
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
        assert_eq!(required_strs, vec!["subject_uri", "subject_cid"]);
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
        assert!(
            props_obj.contains_key("subject_uri"),
            "missing 'subject_uri' property"
        );
        assert!(
            props_obj.contains_key("subject_cid"),
            "missing 'subject_cid' property"
        );
        assert_eq!(props_obj.len(), 2, "expected exactly 2 properties");
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
        assert!(
            props_obj.contains_key("response"),
            "missing 'response' output field"
        );
        assert_eq!(props_obj.len(), 2, "expected exactly 2 output properties");
    }

    #[tokio::test]
    async fn test_execute_missing_subject_uri_returns_invalid_input() {
        let mock = MockBlueskyClient {
            response: serde_json::json!({}),
        };
        let input = serde_json::json!({ "subject_cid": "bafycid" });
        let result = execute(&mock, &input).await;
        assert!(matches!(result.unwrap_err(), BlueskyError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn test_execute_missing_subject_cid_returns_invalid_input() {
        let mock = MockBlueskyClient {
            response: serde_json::json!({}),
        };
        let input = serde_json::json!({ "subject_uri": "at://x" });
        let result = execute(&mock, &input).await;
        assert!(matches!(result.unwrap_err(), BlueskyError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn test_execute_extracts_uri_from_response() {
        let mock = MockBlueskyClient {
            response: serde_json::json!({
                "uri": "at://did:plc:abc123/app.bsky.feed.repost/3k2la",
                "cid": "bafyrepost"
            }),
        };

        let input = serde_json::json!({
            "subject_uri": "at://did:plc:abc123/app.bsky.feed.post/target",
            "subject_cid": "bafytarget"
        });

        let result = execute(&mock, &input).await.unwrap();
        assert!(result.success);
        assert_eq!(
            result.output["uri"],
            "at://did:plc:abc123/app.bsky.feed.repost/3k2la"
        );
        assert!(result.message.contains("reposted"));
    }

    #[tokio::test]
    async fn test_execute_handles_missing_fields_in_response() {
        let mock = MockBlueskyClient {
            response: serde_json::json!({ "validationStatus": "valid" }),
        };

        let input = serde_json::json!({
            "subject_uri": "at://x",
            "subject_cid": "c"
        });

        let result = execute(&mock, &input).await.unwrap();
        assert!(result.success);
        assert_eq!(result.output["uri"], "");
    }
}
