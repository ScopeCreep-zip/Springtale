//! Active-discovery action — fetches the bot's NIP-02 Kind 3 contact
//! list from configured relays and enumerates the `p` tags.
//!
//! Each contact becomes a `nostr://pubkey/{hex}` workspace key. The
//! display name falls back to a truncated pubkey when no NIP-02 alias
//! is set.

use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;
use springtale_connector::workspace_key;

use crate::client::NostrApi;
use crate::error::NostrError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        read_only: true,
        destructive: None,
        name: "discover_destinations".to_owned(),
        description: "Fetch the bot's NIP-02 Kind 3 contact list and emit one row per follow."
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
    client: &dyn NostrApi,
    _input: &serde_json::Value,
) -> Result<ActionResult, NostrError> {
    let contacts = client.list_destinations().await?;
    let mut rows = Vec::with_capacity(contacts.len());
    for c in &contacts {
        let workspace_key = workspace_key::build("nostr", &["pubkey", &c.pubkey_hex]);
        let display = c.alias.clone().unwrap_or_else(|| {
            let hex = &c.pubkey_hex;
            if hex.len() >= 12 {
                format!("{}…{}", &hex[..8], &hex[hex.len() - 4..])
            } else {
                hex.clone()
            }
        });
        let mut metadata = serde_json::Map::new();
        metadata.insert(
            "pubkey_hex".to_owned(),
            serde_json::Value::String(c.pubkey_hex.clone()),
        );
        if let Some(a) = &c.alias {
            metadata.insert("alias".to_owned(), serde_json::Value::String(a.clone()));
        }
        rows.push(serde_json::json!({
            "workspace_key": workspace_key,
            "display_name": display,
            "kind": "pubkey",
            "metadata": serde_json::Value::Object(metadata),
        }));
    }
    let count = rows.len();
    Ok(ActionResult {
        success: true,
        output: serde_json::json!({ "workspaces": rows }),
        message: format!("discovered {count} destination(s)"),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::client::test_helpers::MockNostrApi;

    fn mock() -> MockNostrApi {
        MockNostrApi {
            response_id: "fake".to_owned(),
        }
    }

    #[test]
    fn test_declaration_name() {
        assert_eq!(declaration().name, "discover_destinations");
    }

    #[tokio::test]
    async fn test_execute_returns_three_pubkeys() {
        let mock = mock();
        let result = execute(&mock, &serde_json::json!({})).await.unwrap();
        let arr = result.output["workspaces"].as_array().unwrap();
        assert_eq!(arr.len(), 3);
    }

    #[tokio::test]
    async fn test_execute_uri_uses_pubkey_segment() {
        let mock = mock();
        let result = execute(&mock, &serde_json::json!({})).await.unwrap();
        for row in result.output["workspaces"].as_array().unwrap() {
            assert!(
                row["workspace_key"]
                    .as_str()
                    .unwrap()
                    .starts_with("nostr://pubkey/")
            );
        }
    }

    #[tokio::test]
    async fn test_execute_falls_back_to_truncated_pubkey_when_no_alias() {
        let mock = mock();
        let result = execute(&mock, &serde_json::json!({})).await.unwrap();
        let row_no_alias = result.output["workspaces"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| !r["metadata"].as_object().unwrap().contains_key("alias"))
            .unwrap();
        let display = row_no_alias["display_name"].as_str().unwrap();
        assert!(display.contains("…"), "got: {display}");
    }
}
