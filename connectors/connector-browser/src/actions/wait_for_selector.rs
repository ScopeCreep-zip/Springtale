//! `wait_for_selector` — poll until an element appears or the
//! timeout elapses.
//!
//! Useful before `query_all` / `extract_text` on pages that hydrate
//! asynchronously (SPA frameworks, lazy-loaded grids). Returns
//! `found: false` on timeout — the action itself succeeds, the
//! recipe checks the boolean and decides how to branch.

use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::client::BrowserApi;
use crate::error::BrowserError;

const DEFAULT_TIMEOUT_MS: u32 = 5_000;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        read_only: true,
        destructive: None,
        poll_interval_secs: None,
        name: "wait_for_selector".to_owned(),
        description: "Wait for a CSS selector to appear in the DOM, up to `timeout_ms` \
             (default 5000). Returns `{ found: bool }`. Does not error on \
             timeout — recipes check the boolean explicitly."
            .to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "selector": { "type": "string", "description": "CSS selector." },
                "timeout_ms": {
                    "type": "integer",
                    "minimum": 100,
                    "maximum": 60000,
                    "default": DEFAULT_TIMEOUT_MS,
                    "description": "Maximum wait, in milliseconds."
                }
            },
            "required": ["selector"]
        })),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "found": { "type": "boolean" }
            },
            "required": ["found"]
        })),
    }
}

pub async fn execute(
    client: &dyn BrowserApi,
    input: &serde_json::Value,
) -> Result<ActionResult, BrowserError> {
    let selector = input
        .get("selector")
        .and_then(|v| v.as_str())
        .ok_or_else(|| BrowserError::InvalidInput("missing 'selector'".into()))?;
    let timeout_ms = input
        .get("timeout_ms")
        .and_then(|v| v.as_u64())
        .map(|n| n as u32)
        .unwrap_or(DEFAULT_TIMEOUT_MS);

    let found = client.wait_for_selector(selector, timeout_ms).await?;

    Ok(ActionResult {
        success: true,
        output: serde_json::json!({ "found": found }),
        message: if found {
            format!("`{selector}` appeared within {timeout_ms}ms")
        } else {
            format!("`{selector}` not found within {timeout_ms}ms")
        },
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::client::test_helpers::MockBrowserApi;

    #[tokio::test]
    async fn test_wait_for_selector_found() {
        let client = MockBrowserApi;
        let input = serde_json::json!({ "selector": ".loaded", "timeout_ms": 1000 });
        let result = execute(&client, &input).await.unwrap();
        assert!(result.success);
        assert_eq!(result.output["found"], true);
    }

    #[tokio::test]
    async fn test_wait_for_selector_default_timeout() {
        let client = MockBrowserApi;
        let input = serde_json::json!({ "selector": ".loaded" });
        let result = execute(&client, &input).await.unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_wait_for_selector_missing_selector() {
        let client = MockBrowserApi;
        let err = execute(&client, &serde_json::json!({})).await.unwrap_err();
        assert!(matches!(err, BrowserError::InvalidInput(_)));
    }
}
