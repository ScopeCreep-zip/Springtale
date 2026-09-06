//! `springtale send` — one message out through a connector.

use anyhow::Result;
use serde_json::{Value, json};

use crate::client::Client;
use crate::output;

/// Send one message on `connector`/`target`.
pub async fn run(
    connector: String,
    target: String,
    text: String,
    json_out: bool,
) -> Result<()> {
    let client = Client::from_config()?;
    let body: Value = client
        .post(
            "/send",
            &json!({ "connector": connector, "target": target, "text": text }),
        )
        .await?;
    output::emit(json_out, &body, |v| {
        format!(
            "{} -> {} ({})",
            connector,
            target,
            output::cell(v, "status")
        )
    })
}
