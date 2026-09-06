use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::client::{OpenCodeApi, extract_reply_text};
use crate::error::OpenCodeError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        // Agentic coding edits files and runs commands on the host — never
        // read-only. The W2 chat-approval gate fronts it.
        read_only: false,
        destructive: None,
        poll_interval_secs: None,
        name: "run_task".to_owned(),
        description: "Start a new agentic coding task: create a session and send the prompt to the opencode agent.".to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "prompt": { "type": "string", "description": "The coding task, in plain language." },
                "title": { "type": "string", "description": "Optional session title for later reference." }
            },
            "required": ["prompt"]
        })),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "session_id": { "type": "string" },
                "reply": { "type": "string", "description": "The agent's text reply." },
                "response": { "type": "object", "description": "Raw message response (info + parts)." }
            },
            "required": ["session_id", "reply"]
        })),
    }
}

pub async fn execute(
    client: &dyn OpenCodeApi,
    input: &serde_json::Value,
) -> Result<ActionResult, OpenCodeError> {
    let prompt = input
        .get("prompt")
        .and_then(|v| v.as_str())
        .ok_or_else(|| OpenCodeError::InvalidInput("missing 'prompt'".to_owned()))?;
    let title = input.get("title").and_then(|v| v.as_str());

    let session_id = client.create_session(title).await?;
    let response = client.send_prompt(&session_id, prompt).await?;
    let reply = extract_reply_text(&response);

    Ok(ActionResult {
        success: true,
        output: serde_json::json!({
            "session_id": session_id,
            "reply": reply,
            "response": response,
        }),
        message: format!("opencode task started (session {session_id})"),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::actions::test_support::MockOpenCodeClient;

    #[tokio::test]
    async fn run_task_creates_session_and_returns_reply() {
        let client = MockOpenCodeClient::new(
            "sess-1",
            serde_json::json!({
                "info": { "id": "m1" },
                "parts": [ { "type": "text", "text": "Done." } ]
            }),
        );
        let input = serde_json::json!({ "prompt": "fix the off-by-one" });
        let result = execute(&client, &input).await.unwrap();
        assert!(result.success);
        assert_eq!(result.output["session_id"], "sess-1");
        assert_eq!(result.output["reply"], "Done.");
    }

    #[tokio::test]
    async fn run_task_missing_prompt_errors() {
        let client = MockOpenCodeClient::new("sess-1", serde_json::json!({}));
        let input = serde_json::json!({ "title": "x" });
        assert!(execute(&client, &input).await.is_err());
    }
}
