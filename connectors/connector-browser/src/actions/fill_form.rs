use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::client::BrowserApi;
use crate::error::BrowserError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        read_only: false,
        destructive: None,
        poll_interval_secs: None,
        name: "fill_form".to_owned(),
        description: "Fill a form field by CSS selector.".to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "selector": { "type": "string", "description": "CSS selector for the input field" },
                "value": { "type": "string", "description": "Value to fill" }
            },
            "required": ["selector", "value"]
        })),
        output_schema: None,
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
    let value = input
        .get("value")
        .and_then(|v| v.as_str())
        .ok_or_else(|| BrowserError::InvalidInput("missing 'value'".into()))?;

    client.fill_form(selector, value).await?;

    Ok(ActionResult {
        success: true,
        output: serde_json::json!({}),
        message: format!("filled {selector}"),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::client::test_helpers::MockBrowserApi;

    #[tokio::test]
    async fn test_fill_form_success() {
        let client = MockBrowserApi;
        let input = serde_json::json!({ "selector": "#email", "value": "test@example.com" });
        let result = execute(&client, &input).await.unwrap();
        assert!(result.success);
    }
}
