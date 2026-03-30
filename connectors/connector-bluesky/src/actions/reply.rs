use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::client::BlueskyApi;
use crate::error::BlueskyError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        name: "reply".to_owned(),
        description: "Reply to a Bluesky post.".to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "text": { "type": "string", "description": "Reply text." },
                "parent_uri": { "type": "string", "description": "AT URI of the parent post." },
                "parent_cid": { "type": "string", "description": "CID of the parent post." },
                "root_uri": { "type": "string", "description": "AT URI of the root post in the thread." },
                "root_cid": { "type": "string", "description": "CID of the root post." }
            },
            "required": ["text", "parent_uri", "parent_cid", "root_uri", "root_cid"]
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
    let text = input.get("text").and_then(|v| v.as_str())
        .ok_or_else(|| BlueskyError::InvalidInput("missing 'text'".to_owned()))?;
    let parent_uri = input.get("parent_uri").and_then(|v| v.as_str())
        .ok_or_else(|| BlueskyError::InvalidInput("missing 'parent_uri'".to_owned()))?;
    let parent_cid = input.get("parent_cid").and_then(|v| v.as_str())
        .ok_or_else(|| BlueskyError::InvalidInput("missing 'parent_cid'".to_owned()))?;
    let root_uri = input.get("root_uri").and_then(|v| v.as_str())
        .ok_or_else(|| BlueskyError::InvalidInput("missing 'root_uri'".to_owned()))?;
    let root_cid = input.get("root_cid").and_then(|v| v.as_str())
        .ok_or_else(|| BlueskyError::InvalidInput("missing 'root_cid'".to_owned()))?;

    let response = client.reply(text, parent_uri, parent_cid, root_uri, root_cid).await?;

    let uri = response.get("uri").and_then(|v| v.as_str()).unwrap_or_default();
    let cid = response.get("cid").and_then(|v| v.as_str()).unwrap_or_default();

    Ok(ActionResult {
        success: true,
        output: serde_json::json!({
            "uri": uri,
            "cid": cid,
            "response": response,
        }),
        message: format!("replied to {parent_uri}"),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// Mock client that returns canned ATProto createRecord responses.
    struct MockBlueskyClient {
        response: serde_json::Value,
    }

    #[async_trait::async_trait]
    impl BlueskyApi for MockBlueskyClient {
        async fn create_post(&self, _text: &str) -> Result<serde_json::Value, BlueskyError> {
            Ok(self.response.clone())
        }

        async fn reply(
            &self,
            _text: &str,
            _parent_uri: &str,
            _parent_cid: &str,
            _root_uri: &str,
            _root_cid: &str,
        ) -> Result<serde_json::Value, BlueskyError> {
            Ok(self.response.clone())
        }

        async fn like(
            &self,
            _subject_uri: &str,
            _subject_cid: &str,
        ) -> Result<serde_json::Value, BlueskyError> {
            Ok(self.response.clone())
        }

        async fn repost(
            &self,
            _subject_uri: &str,
            _subject_cid: &str,
        ) -> Result<serde_json::Value, BlueskyError> {
            Ok(self.response.clone())
        }
    }

    #[test]
    fn test_declaration_name() {
        let decl = declaration();
        assert_eq!(decl.name, "reply");
    }

    #[test]
    fn test_declaration_input_schema_required_fields() {
        let decl = declaration();
        let schema = decl.input_schema.as_ref().unwrap_or_else(|| panic!("input_schema is None"));
        let required = schema.get("required").unwrap_or_else(|| panic!("missing required"));
        let required_arr = required.as_array().unwrap_or_else(|| panic!("required not array"));
        let required_strs: Vec<&str> = required_arr
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert_eq!(
            required_strs,
            vec!["text", "parent_uri", "parent_cid", "root_uri", "root_cid"]
        );
        assert_eq!(required_strs.len(), 5, "reply requires exactly 5 parameters");
    }

    #[test]
    fn test_declaration_input_schema_properties() {
        let decl = declaration();
        let schema = decl.input_schema.as_ref().unwrap_or_else(|| panic!("input_schema is None"));
        let props = schema.get("properties").unwrap_or_else(|| panic!("missing properties"));
        let props_obj = props.as_object().unwrap_or_else(|| panic!("properties not object"));
        let expected_keys = ["text", "parent_uri", "parent_cid", "root_uri", "root_cid"];
        for key in &expected_keys {
            assert!(props_obj.contains_key(*key), "missing '{key}' property");
        }
        assert_eq!(props_obj.len(), 5, "expected exactly 5 properties");
    }

    #[test]
    fn test_declaration_output_schema_fields() {
        let decl = declaration();
        let schema = decl.output_schema.as_ref().unwrap_or_else(|| panic!("output_schema is None"));
        let props = schema.get("properties").unwrap_or_else(|| panic!("missing properties"));
        let props_obj = props.as_object().unwrap_or_else(|| panic!("properties not object"));
        assert!(props_obj.contains_key("uri"), "missing 'uri' output field");
        assert!(props_obj.contains_key("cid"), "missing 'cid' output field");
        assert!(props_obj.contains_key("response"), "missing 'response' output field");
        assert_eq!(props_obj.len(), 3, "expected exactly 3 output properties");
    }

    #[tokio::test]
    async fn test_execute_missing_text_returns_invalid_input() {
        let mock = MockBlueskyClient {
            response: serde_json::json!({}),
        };
        let input = serde_json::json!({
            "parent_uri": "at://x", "parent_cid": "c",
            "root_uri": "at://x", "root_cid": "c"
        });
        let result = execute(&mock, &input).await;
        assert!(matches!(result.unwrap_err(), BlueskyError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn test_execute_missing_parent_uri_returns_invalid_input() {
        let mock = MockBlueskyClient {
            response: serde_json::json!({}),
        };
        let input = serde_json::json!({
            "text": "hi", "parent_cid": "c",
            "root_uri": "at://x", "root_cid": "c"
        });
        let result = execute(&mock, &input).await;
        assert!(matches!(result.unwrap_err(), BlueskyError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn test_execute_extracts_uri_and_cid_from_response() {
        let mock = MockBlueskyClient {
            response: serde_json::json!({
                "uri": "at://did:plc:abc123/app.bsky.feed.post/reply1",
                "cid": "bafyreireply"
            }),
        };

        let input = serde_json::json!({
            "text": "reply text",
            "parent_uri": "at://did:plc:abc123/app.bsky.feed.post/parent1",
            "parent_cid": "bafyparent",
            "root_uri": "at://did:plc:abc123/app.bsky.feed.post/root1",
            "root_cid": "bafyroot"
        });

        let result = execute(&mock, &input).await.unwrap();
        assert!(result.success);
        assert_eq!(result.output["uri"], "at://did:plc:abc123/app.bsky.feed.post/reply1");
        assert_eq!(result.output["cid"], "bafyreireply");
        assert!(result.message.contains("parent1"));
    }

    #[tokio::test]
    async fn test_execute_handles_missing_fields_in_response() {
        let mock = MockBlueskyClient {
            response: serde_json::json!({ "validationStatus": "valid" }),
        };

        let input = serde_json::json!({
            "text": "t", "parent_uri": "at://x", "parent_cid": "c",
            "root_uri": "at://r", "root_cid": "c"
        });

        let result = execute(&mock, &input).await.unwrap();
        assert!(result.success);
        assert_eq!(result.output["uri"], "");
        assert_eq!(result.output["cid"], "");
    }
}
