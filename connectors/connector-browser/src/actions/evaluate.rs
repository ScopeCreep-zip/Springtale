//! `evaluate` — run a JavaScript expression in the current page and
//! return the result as JSON.
//!
//! Power-user primitive — most recipes prefer `get_html` +
//! `extraction` for declarative scraping. `evaluate` is reserved for
//! cases the declarative ladder can't reach: clicking a deep
//! Shadow-DOM element, reading a JS-only data structure, executing
//! a site's own helper functions.
//!
//! Security: the JS runs in the page context with access to its
//! cookies / localStorage. The domain allow-list enforced at
//! `navigate` time still bounds which origins can be touched —
//! `evaluate` doesn't escape that perimeter.

use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::client::BrowserApi;
use crate::error::BrowserError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        read_only: false,
        destructive: None,
        name: "evaluate".to_owned(),
        description:
            "Run a JavaScript expression in the current page and return the result as JSON. \
             Wrap multi-statement logic in an IIFE: `(() => { … })()`."
                .to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "js": {
                    "type": "string",
                    "description": "JavaScript expression evaluating to a JSON-serializable value."
                }
            },
            "required": ["js"]
        })),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "value": {
                    "description": "The deserialized return value. `null` when the JS returned `undefined`."
                }
            }
        })),
    }
}

pub async fn execute(
    client: &dyn BrowserApi,
    input: &serde_json::Value,
) -> Result<ActionResult, BrowserError> {
    let js = input
        .get("js")
        .and_then(|v| v.as_str())
        .ok_or_else(|| BrowserError::InvalidInput("missing 'js'".into()))?;

    let value = client.evaluate(js).await?;

    Ok(ActionResult {
        success: true,
        output: serde_json::json!({ "value": value }),
        message: format!("evaluated {} chars", js.len()),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::client::test_helpers::MockBrowserApi;

    #[tokio::test]
    async fn test_evaluate_success() {
        let client = MockBrowserApi;
        let input = serde_json::json!({ "js": "1 + 1" });
        let result = execute(&client, &input).await.unwrap();
        assert!(result.success);
        assert!(result.output.get("value").is_some());
    }

    #[tokio::test]
    async fn test_evaluate_missing_js() {
        let client = MockBrowserApi;
        let input = serde_json::json!({});
        let err = execute(&client, &input).await.unwrap_err();
        assert!(matches!(err, BrowserError::InvalidInput(_)));
    }
}
