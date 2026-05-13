//! Active-discovery action — returns the connector's own Bluesky account
//! as a single addressable workspace.
//!
//! Bluesky's surface is asymmetric: posts go to the bot's own feed, not
//! to a destination chosen at send time. So enumeration here returns
//! exactly one row — the authenticated account — under the
//! `bluesky://account/{did}` URI scheme. The frontend picker can still
//! offer this account as a "destination" so the universal recipe shape
//! (`destination = WorkspaceTarget(...)`) holds across every messaging
//! connector.

use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;
use springtale_connector::workspace_key;

use crate::client::BlueskyApi;
use crate::error::BlueskyError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        name: "discover_destinations".to_owned(),
        description:
            "Enumerate addressable destinations for this Bluesky session — returns the authenticated account as a single row."
                .to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {}
        })),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "workspaces": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "workspace_key": { "type": "string" },
                            "display_name":  { "type": "string" },
                            "kind":          { "type": "string" },
                            "metadata":      { "type": "object" }
                        }
                    }
                }
            }
        })),
    }
}

pub async fn execute(
    client: &dyn BlueskyApi,
    _input: &serde_json::Value,
) -> Result<ActionResult, BlueskyError> {
    let (did, handle) = client.current_account().await?;

    let workspace_key = workspace_key::build("bluesky", &["account", &did]);
    let mut metadata = serde_json::Map::new();
    metadata.insert("did".to_owned(), serde_json::Value::String(did.clone()));
    metadata.insert(
        "handle".to_owned(),
        serde_json::Value::String(handle.clone()),
    );

    let row = serde_json::json!({
        "workspace_key": workspace_key,
        "display_name": format!("@{handle}"),
        "kind": "account",
        "metadata": serde_json::Value::Object(metadata),
    });

    Ok(ActionResult {
        success: true,
        output: serde_json::json!({ "workspaces": [row] }),
        message: format!("discovered 1 destination (@{handle})"),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::client::test_helpers::MockBlueskyClient;

    #[test]
    fn test_declaration_name() {
        assert_eq!(declaration().name, "discover_destinations");
    }

    #[test]
    fn test_declaration_output_lists_workspaces_array() {
        let decl = declaration();
        let schema = decl.output_schema.unwrap();
        let props = schema
            .get("properties")
            .and_then(|v| v.as_object())
            .unwrap();
        assert!(props.contains_key("workspaces"));
    }

    #[tokio::test]
    async fn test_execute_returns_authenticated_account() {
        let mock = MockBlueskyClient {
            response: serde_json::json!({}),
        };
        let result = execute(&mock, &serde_json::json!({})).await.unwrap();
        let arr = result
            .output
            .get("workspaces")
            .and_then(|v| v.as_array())
            .unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(
            arr[0].get("workspace_key").and_then(|v| v.as_str()).unwrap(),
            "bluesky://account/did:plc:mocktestaccount"
        );
        assert_eq!(
            arr[0].get("kind").and_then(|v| v.as_str()).unwrap(),
            "account"
        );
        assert_eq!(
            arr[0].get("display_name").and_then(|v| v.as_str()).unwrap(),
            "@mock.bsky.social"
        );
    }

    #[tokio::test]
    async fn test_execute_metadata_contains_did_and_handle() {
        let mock = MockBlueskyClient {
            response: serde_json::json!({}),
        };
        let result = execute(&mock, &serde_json::json!({})).await.unwrap();
        let row = &result.output["workspaces"][0];
        let metadata = row.get("metadata").and_then(|v| v.as_object()).unwrap();
        assert_eq!(
            metadata.get("did").and_then(|v| v.as_str()).unwrap(),
            "did:plc:mocktestaccount"
        );
        assert_eq!(
            metadata.get("handle").and_then(|v| v.as_str()).unwrap(),
            "mock.bsky.social"
        );
    }
}
