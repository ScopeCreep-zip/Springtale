//! `springtale chat` — inject a message into the bot runtime.

use anyhow::Result;
use serde_json::{Value, json};

use crate::client::Client;
use crate::output;

/// Send one chat message.
pub async fn run(message: String, session: Option<String>, json_out: bool) -> Result<()> {
    let client = Client::from_config()?;
    let body: Value = client
        .post("/chat", &json!({ "text": message, "session": session }))
        .await?;
    output::emit(json_out, &body, chat_line)
}

/// The `chat` acknowledgement line.
fn chat_line(v: &Value) -> String {
    format!(
        "{} (session {})",
        output::cell(v, "status"),
        output::cell(v, "session")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::{json_value, key_set};

    #[test]
    fn test_chat_json_shape_has_status_and_session() {
        let body = json!({ "status": "queued", "session": "s-1" });
        let out = json_value(&body);
        assert_eq!(key_set(&out), ["session", "status"]);
        assert!(out["status"].is_string());
        assert!(out["session"].is_string());
        assert_eq!(chat_line(&body), "queued (session s-1)");
    }

    #[test]
    fn test_chat_line_leaves_missing_fields_blank_rather_than_panicking() {
        assert_eq!(chat_line(&json!({})), " (session )");
    }
}
