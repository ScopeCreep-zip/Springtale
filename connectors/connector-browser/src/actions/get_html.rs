//! `get_html` — return the full rendered HTML of the current page.
//!
//! Primary input to the extraction ladder (`springtale-runtime::extraction`).
//! Recipes pipe `${last_connector_output.html}` into an `Action::Extract`
//! step (Readability / CSS / PageDiff) for declarative scraping.

use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::client::BrowserApi;
use crate::error::BrowserError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        read_only: true,
        name: "get_html".to_owned(),
        description:
            "Return the full rendered HTML of the current page (post-JavaScript execution). \
             Call `navigate` first to load a URL."
                .to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        })),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "html": {
                    "type": "string",
                    "description": "Rendered HTML of the current page."
                },
                "bytes": {
                    "type": "integer",
                    "description": "Byte length of the HTML (sizes-only telemetry)."
                }
            },
            "required": ["html"]
        })),
    }
}

pub async fn execute(
    client: &dyn BrowserApi,
    _input: &serde_json::Value,
) -> Result<ActionResult, BrowserError> {
    let html = client.get_html().await?;
    let bytes = html.len();
    Ok(ActionResult {
        success: true,
        output: serde_json::json!({ "html": html, "bytes": bytes }),
        message: format!("fetched {bytes} bytes of HTML"),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::client::test_helpers::MockBrowserApi;

    #[tokio::test]
    async fn test_get_html_success() {
        let client = MockBrowserApi;
        let result = execute(&client, &serde_json::json!({})).await.unwrap();
        assert!(result.success);
        assert!(result.output.get("html").is_some());
        assert_eq!(result.output["bytes"], 30); // mock HTML length
    }
}
