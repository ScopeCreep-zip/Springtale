use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::client::BrowserApi;
use crate::error::BrowserError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        read_only: true,
        destructive: None,
        poll_interval_secs: None,
        name: "extract_text".to_owned(),
        description: "Extract text content from an element by CSS selector.".to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "selector": { "type": "string", "description": "CSS selector for the element" }
            },
            "required": ["selector"]
        })),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "text": { "type": "string" }
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

    let text = client.extract_text(selector).await?;

    Ok(ActionResult {
        success: true,
        output: serde_json::json!({ "text": text }),
        message: format!("extracted text from {selector}"),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::client::test_helpers::MockBrowserApi;

    #[tokio::test]
    async fn test_extract_text_success() {
        let client = MockBrowserApi;
        let input = serde_json::json!({ "selector": "h1" });
        let result = execute(&client, &input).await.unwrap();
        assert!(result.success);
    }
}
