use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::client::BlueskyApi;
use crate::error::BlueskyError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        read_only: false,
        destructive: None,
        poll_interval_secs: None,
        name: "reply".to_owned(),
        description: "Reply to a Bluesky post.".to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "text": { "type": "string", "description": "Reply text." },
                "parent_uri": { "type": "string", "description": "AT URI of the parent post." },
                "parent_cid": { "type": "string", "description": "CID of the parent post." },
                "root_uri": { "type": "string", "description": "AT URI of the root post in the thread — pass the `mention` trigger's `root_uri`, not the mentioned post's `uri`." },
                "root_cid": { "type": "string", "description": "CID of the root post — pass the `mention` trigger's `root_cid`." }
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
    let text = input
        .get("text")
        .and_then(|v| v.as_str())
        .ok_or_else(|| BlueskyError::InvalidInput("missing 'text'".to_owned()))?;
    let parent_uri = input
        .get("parent_uri")
        .and_then(|v| v.as_str())
        .ok_or_else(|| BlueskyError::InvalidInput("missing 'parent_uri'".to_owned()))?;
    let parent_cid = input
        .get("parent_cid")
        .and_then(|v| v.as_str())
        .ok_or_else(|| BlueskyError::InvalidInput("missing 'parent_cid'".to_owned()))?;
    let root_uri = input
        .get("root_uri")
        .and_then(|v| v.as_str())
        .ok_or_else(|| BlueskyError::InvalidInput("missing 'root_uri'".to_owned()))?;
    let root_cid = input
        .get("root_cid")
        .and_then(|v| v.as_str())
        .ok_or_else(|| BlueskyError::InvalidInput("missing 'root_cid'".to_owned()))?;

    let response = client
        .reply(text, parent_uri, parent_cid, root_uri, root_cid)
        .await?;

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
        message: format!("replied to {parent_uri}"),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    use crate::client::test_helpers::{MockBlueskyClient, RecordingBlueskyClient};
    use crate::gateway::route_jetstream_event;

    const OWN: &str = "did:plc:me";

    /// A real Jetstream `app.bsky.feed.post` create commit that mentions
    /// us. `reply` is the record's own reply ref (`None` for a top-level
    /// post), which is what decides the thread root.
    fn mention_commit(reply: Option<serde_json::Value>) -> serde_json::Value {
        let mut record = serde_json::json!({
            "$type": "app.bsky.feed.post",
            "text": "hey @me",
            "facets": [{
                "features": [{ "$type": "app.bsky.richtext.facet#mention", "did": OWN }],
                "index": { "byteStart": 4, "byteEnd": 7 }
            }]
        });
        if let Some(r) = reply {
            record["reply"] = r;
        }
        serde_json::json!({
            "did": "did:plc:someone",
            "time_us": 1_700_000_000_000_000u64,
            "kind": "commit",
            "commit": {
                "operation": "create",
                "collection": "app.bsky.feed.post",
                "rkey": "3kxyz",
                "cid": "bafymention",
                "record": record
            }
        })
    }

    /// The wiring the `bluesky-mention-auto-ack` builtin recipe performs:
    /// parent from the mentioned post, root from the trigger's root.
    fn reply_input_from_mention(payload: &serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "text": "ack",
            "parent_uri": payload["uri"],
            "parent_cid": payload["cid"],
            "root_uri": payload["root_uri"],
            "root_cid": payload["root_cid"],
        })
    }

    #[tokio::test]
    async fn test_reply_to_top_level_mention_roots_at_that_post() {
        let payload = route_jetstream_event(&mention_commit(None), OWN)
            .unwrap_or_else(|| panic!("mention routes"));
        let client = RecordingBlueskyClient::default();

        execute(&client, &reply_input_from_mention(&payload))
            .await
            .unwrap_or_else(|e| panic!("reply failed: {e}"));

        let sent = client
            .captured()
            .unwrap_or_else(|| panic!("reply never reached the client"));
        assert_eq!(
            sent.parent_uri,
            "at://did:plc:someone/app.bsky.feed.post/3kxyz"
        );
        assert_eq!(sent.parent_cid, "bafymention");
        // A top-level mention is the root of its own thread.
        assert_eq!(sent.root_uri, sent.parent_uri);
        assert_eq!(sent.root_cid, sent.parent_cid);
    }

    #[tokio::test]
    async fn test_reply_to_nested_mention_roots_at_thread_root() {
        let commit = mention_commit(Some(serde_json::json!({
            "root": { "uri": "at://did:plc:opener/app.bsky.feed.post/root", "cid": "bafyroot" },
            "parent": { "uri": "at://did:plc:other/app.bsky.feed.post/mid", "cid": "bafymid" }
        })));
        let payload =
            route_jetstream_event(&commit, OWN).unwrap_or_else(|| panic!("mention routes"));
        let client = RecordingBlueskyClient::default();

        execute(&client, &reply_input_from_mention(&payload))
            .await
            .unwrap_or_else(|e| panic!("reply failed: {e}"));

        let sent = client
            .captured()
            .unwrap_or_else(|| panic!("reply never reached the client"));
        // Parent is still the post that mentioned us...
        assert_eq!(
            sent.parent_uri,
            "at://did:plc:someone/app.bsky.feed.post/3kxyz"
        );
        // ...but the thread roots where the conversation started, not at
        // the mention — otherwise clients file the reply as its own thread.
        assert_eq!(sent.root_uri, "at://did:plc:opener/app.bsky.feed.post/root");
        assert_eq!(sent.root_cid, "bafyroot");
        assert_ne!(sent.root_uri, sent.parent_uri);
    }

    #[test]
    fn test_declaration_name() {
        let decl = declaration();
        assert_eq!(decl.name, "reply");
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
        assert_eq!(
            required_strs,
            vec!["text", "parent_uri", "parent_cid", "root_uri", "root_cid"]
        );
        assert_eq!(
            required_strs.len(),
            5,
            "reply requires exactly 5 parameters"
        );
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
        let expected_keys = ["text", "parent_uri", "parent_cid", "root_uri", "root_cid"];
        for key in &expected_keys {
            assert!(props_obj.contains_key(*key), "missing '{key}' property");
        }
        assert_eq!(props_obj.len(), 5, "expected exactly 5 properties");
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
        assert_eq!(
            result.output["uri"],
            "at://did:plc:abc123/app.bsky.feed.post/reply1"
        );
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
