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
    output::emit(json_out, &body, |v| {
        format!(
            "{} (session {})",
            output::cell(v, "status"),
            output::cell(v, "session")
        )
    })
}
