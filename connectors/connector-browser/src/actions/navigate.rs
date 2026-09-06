use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::client::BrowserApi;
use crate::error::BrowserError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        read_only: false,
        destructive: None,
        poll_interval_secs: None,
        name: "navigate".to_owned(),
        description: "Navigate to a URL. Domain must be in the connector's allow-list.".to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "url": { "type": "string", "description": "URL to navigate to" }
            },
            "required": ["url"]
        })),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "url": { "type": "string" },
                "status": { "type": "string" }
            }
        })),
    }
}

pub async fn execute(
    client: &dyn BrowserApi,
    input: &serde_json::Value,
) -> Result<ActionResult, BrowserError> {
    let url = input
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| BrowserError::InvalidInput("missing 'url'".into()))?;

    let response = client.navigate(url).await?;

    Ok(ActionResult {
        success: true,
        output: response,
        message: format!("navigated to {url}"),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::client::test_helpers::MockBrowserApi;

    #[tokio::test]
    async fn test_navigate_success() {
        let client = MockBrowserApi;
        let input = serde_json::json!({ "url": "https://example.com" });
        let result = execute(&client, &input).await.unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_navigate_missing_url() {
        let client = MockBrowserApi;
        assert!(execute(&client, &serde_json::json!({})).await.is_err());
    }
}
