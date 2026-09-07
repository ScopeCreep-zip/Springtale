//! `springtale send` — one message out through a connector.

use anyhow::Result;
use serde_json::{Value, json};

use crate::client::Client;
use crate::output;

/// Send one message on `connector`/`target`.
pub async fn run(connector: String, target: String, text: String, json_out: bool) -> Result<()> {
    let client = Client::from_config()?;
    let body: Value = client
        .post(
            "/send",
            &json!({ "connector": connector, "target": target, "text": text }),
        )
        .await?;
    output::emit(json_out, &body, |v| send_line(v, &connector, &target))
}

/// The `send` acknowledgement line.
fn send_line(v: &Value, connector: &str, target: &str) -> String {
    format!(
        "{} -> {} ({})",
        connector,
        target,
        output::cell(v, "status")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::{json_value, key_set};

    #[test]
    fn test_send_json_shape_reports_the_delivery_status() {
        let body = json!({ "status": "sent" });
        let out = json_value(&body);
        assert_eq!(key_set(&out), ["status"]);
        assert!(out["status"].is_string());
        assert_eq!(
            send_line(&body, "telegram", "@channel"),
            "telegram -> @channel (sent)"
        );
    }

    #[test]
    fn test_send_line_leaves_an_absent_status_blank() {
        assert_eq!(send_line(&json!({}), "telegram", "@c"), "telegram -> @c ()");
    }
}
