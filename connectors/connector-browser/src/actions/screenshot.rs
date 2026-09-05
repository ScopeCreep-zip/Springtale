use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::client::BrowserApi;
use crate::error::BrowserError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        read_only: true,
        destructive: None,
        name: "screenshot".to_owned(),
        description: "Capture a screenshot of the current page.".to_owned(),
        input_schema: None,
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "data": { "type": "string", "description": "Base64-encoded PNG" }
            }
        })),
    }
}

pub async fn execute(
    client: &dyn BrowserApi,
    _input: &serde_json::Value,
) -> Result<ActionResult, BrowserError> {
    let data = client.screenshot().await?;

    Ok(ActionResult {
        success: true,
        output: serde_json::json!({ "data": data }),
        message: "screenshot captured".to_owned(),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::client::test_helpers::MockBrowserApi;

    #[tokio::test]
    async fn test_screenshot_success() {
        let client = MockBrowserApi;
        let result = execute(&client, &serde_json::json!({})).await.unwrap();
        assert!(result.success);
    }
}
