//! `query_all` — return every element matching a CSS selector,
//! with text / outer HTML / tag name / attributes per match.
//!
//! One-round-trip primitive: the underlying client evaluates a
//! `document.querySelectorAll(selector).map(...)` snippet in the
//! page so matches return in a single CDP exchange regardless of
//! how many elements were found. Used by the selector picker's
//! pattern-detect mode (B.8) and by recipes that need every match
//! at once (not just the first).

use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::client::BrowserApi;
use crate::error::BrowserError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        name: "query_all".to_owned(),
        description:
            "Return every element matching a CSS selector with its text, outer HTML, \
             tag name, and full attribute map. Empty array when nothing matches."
                .to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "selector": {
                    "type": "string",
                    "description": "CSS selector."
                }
            },
            "required": ["selector"]
        })),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "matches": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "text": { "type": "string" },
                            "html": { "type": "string" },
                            "tag_name": { "type": "string" },
                            "attrs": {
                                "type": "object",
                                "additionalProperties": { "type": "string" }
                            }
                        }
                    }
                },
                "count": { "type": "integer" }
            }
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

    let matches = client.query_all(selector).await?;
    let count = matches.len();

    Ok(ActionResult {
        success: true,
        output: serde_json::json!({ "matches": matches, "count": count }),
        message: format!("matched {count} element(s) for `{selector}`"),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::client::test_helpers::MockBrowserApi;

    #[tokio::test]
    async fn test_query_all_success() {
        let client = MockBrowserApi;
        let input = serde_json::json!({ "selector": "div" });
        let result = execute(&client, &input).await.unwrap();
        assert!(result.success);
        let matches = result.output["matches"].as_array().unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(result.output["count"], 1);
    }

    #[tokio::test]
    async fn test_query_all_missing_selector() {
        let client = MockBrowserApi;
        let err = execute(&client, &serde_json::json!({})).await.unwrap_err();
        assert!(matches!(err, BrowserError::InvalidInput(_)));
    }
}
