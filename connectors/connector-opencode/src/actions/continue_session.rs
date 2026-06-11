use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::client::{OpenCodeApi, extract_reply_text};
use crate::error::OpenCodeError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        read_only: false,
        name: "continue_session".to_owned(),
        description: "Send a follow-up prompt to an existing opencode session (e.g. \"now add tests\").".to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "session_id": { "type": "string", "description": "Session id from a previous run_task." },
                "prompt": { "type": "string", "description": "The follow-up instruction." }
            },
            "required": ["session_id", "prompt"]
        })),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "session_id": { "type": "string" },
                "reply": { "type": "string" },
                "response": { "type": "object" }
            },
            "required": ["session_id", "reply"]
        })),
    }
}

pub async fn execute(
    client: &dyn OpenCodeApi,
    input: &serde_json::Value,
) -> Result<ActionResult, OpenCodeError> {
    let session_id = input
        .get("session_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| OpenCodeError::InvalidInput("missing 'session_id'".to_owned()))?;
    let prompt = input
        .get("prompt")
        .and_then(|v| v.as_str())
        .ok_or_else(|| OpenCodeError::InvalidInput("missing 'prompt'".to_owned()))?;

    let response = client.send_prompt(session_id, prompt).await?;
    let reply = extract_reply_text(&response);

    Ok(ActionResult {
        success: true,
        output: serde_json::json!({
            "session_id": session_id,
            "reply": reply,
            "response": response,
        }),
        message: format!("opencode session {session_id} continued"),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::actions::test_support::MockOpenCodeClient;

    #[tokio::test]
    async fn continue_session_sends_followup() {
        let client = MockOpenCodeClient::new(
            "ignored",
            serde_json::json!({
                "parts": [ { "type": "text", "text": "Tests added." } ]
            }),
        );
        let input = serde_json::json!({ "session_id": "sess-9", "prompt": "now add tests" });
        let result = execute(&client, &input).await.unwrap();
        assert!(result.success);
        assert_eq!(result.output["session_id"], "sess-9");
        assert_eq!(result.output["reply"], "Tests added.");
    }

    #[tokio::test]
    async fn continue_session_missing_id_errors() {
        let client = MockOpenCodeClient::new("x", serde_json::json!({}));
        let input = serde_json::json!({ "prompt": "go" });
        assert!(execute(&client, &input).await.is_err());
    }
}
