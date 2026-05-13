//! Active-discovery action — enumerates groups and 1:1 contacts known
//! to the local signal-cli daemon.
//!
//! Calls `listGroups` then `listContacts` via JSON-RPC. Output rows
//! are workspace keys under the `signal://group/{id}` or
//! `signal://user/{e164}` URI scheme.

use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;
use springtale_connector::workspace_key;

use crate::client::SignalApi;
use crate::error::SignalError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        name: "discover_destinations".to_owned(),
        description: "Enumerate Signal groups (listGroups) and contacts (listContacts) via the local signal-cli daemon."
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
    client: &dyn SignalApi,
    _input: &serde_json::Value,
) -> Result<ActionResult, SignalError> {
    let recipients = client.list_destinations().await?;
    let mut rows = Vec::with_capacity(recipients.len());
    for r in &recipients {
        let (segment, kind) = match r.kind.as_str() {
            "group" => ("group", "group"),
            _ => ("user", "user"),
        };
        let workspace_key = workspace_key::build("signal", &[segment, &r.id]);
        let mut metadata = serde_json::Map::new();
        metadata.insert("id".to_owned(), serde_json::Value::String(r.id.clone()));
        if let Some(m) = r.member_count {
            metadata.insert("member_count".to_owned(), serde_json::Value::from(m));
        }
        rows.push(serde_json::json!({
            "workspace_key": workspace_key,
            "display_name": r.display_name.clone(),
            "kind": kind,
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
    use crate::client::test_helpers::MockSignalApi;

    #[test]
    fn test_declaration_name() {
        assert_eq!(declaration().name, "discover_destinations");
    }

    #[tokio::test]
    async fn test_execute_returns_mixed_groups_and_contacts() {
        let mock = MockSignalApi;
        let result = execute(&mock, &serde_json::json!({})).await.unwrap();
        let arr = result.output["workspaces"].as_array().unwrap();
        assert_eq!(arr.len(), 4);
        let kinds: Vec<&str> = arr
            .iter()
            .map(|r| r["kind"].as_str().unwrap())
            .collect();
        assert!(kinds.contains(&"group"));
        assert!(kinds.contains(&"user"));
    }

    #[tokio::test]
    async fn test_execute_group_uses_signal_group_uri() {
        let mock = MockSignalApi;
        let result = execute(&mock, &serde_json::json!({})).await.unwrap();
        let group = result.output["workspaces"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["kind"] == "group")
            .unwrap();
        assert!(
            group["workspace_key"]
                .as_str()
                .unwrap()
                .starts_with("signal://group/")
        );
    }

    #[tokio::test]
    async fn test_execute_contact_uses_signal_user_uri() {
        let mock = MockSignalApi;
        let result = execute(&mock, &serde_json::json!({})).await.unwrap();
        let user = result.output["workspaces"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["kind"] == "user")
            .unwrap();
        assert!(
            user["workspace_key"]
                .as_str()
                .unwrap()
                .starts_with("signal://user/")
        );
    }
}
