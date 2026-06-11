//! `onboard_url` — D1 Telegram deep-link generator.
//!
//! Returns `https://t.me/{bot_username}?start={payload}` so the
//! frontend's `WorkspaceTargetPicker` 🎯 Onboard button can paste
//! the link into the clipboard. The user taps it, Telegram sends
//! `/start {payload}` to the bot, the polling loop emits
//! `command_received`, and the universal harvester upserts the
//! chat into `mental_model_workspaces`.
//!
//! ## Why an action and not a config field
//!
//! The bot_username depends on the bot token — Telegram returns
//! it from `getMe` once the token is configured. Putting
//! generation in an action keeps the connector self-contained
//! (no extra storage shape, no config edits required) and lets
//! the picker call this action via the same dispatch path it
//! uses for everything else.
//!
//! ## Privacy
//!
//! The deep link contains the bot username (already public, that's
//! its whole point) plus an optional payload string the user
//! supplies. We never include secrets, never include the token.

use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::client::TelegramApi;
use crate::error::TelegramError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        read_only: true,
        name: "onboard_url".to_owned(),
        description: "Build a `https://t.me/<bot>?start=<payload>` deep link. The user \
             taps the link, Telegram sends `/start <payload>` to the bot, and \
             the universal harvester registers the chat in this formation's \
             external-workspace directory."
            .to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "payload": {
                    "type": "string",
                    "description": "Optional payload — typically the formation id, used by the recipe deploy form to route the `/start` to the right registration."
                }
            }
        })),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "url": { "type": "string" },
                "bot_username": { "type": "string" }
            },
            "required": ["url", "bot_username"]
        })),
    }
}

pub async fn execute(
    client: &dyn TelegramApi,
    input: &serde_json::Value,
) -> Result<ActionResult, TelegramError> {
    let payload = input.get("payload").and_then(|v| v.as_str()).unwrap_or("");
    let me = client.get_me().await?;
    // Telegram's getMe returns { ok: true, result: { username, ... } }
    // The TelegramApi trait returns the raw response; the action
    // unwraps the `result` object before reading the username.
    let username = me
        .get("result")
        .and_then(|r| r.get("username"))
        .or_else(|| me.get("username"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            TelegramError::InvalidInput(
                "getMe response missing username — is the token valid?".into(),
            )
        })?;
    let url = if payload.is_empty() {
        format!("https://t.me/{username}")
    } else {
        format!("https://t.me/{username}?start={payload}")
    };
    Ok(ActionResult {
        success: true,
        output: serde_json::json!({
            "url": url,
            "bot_username": username,
        }),
        message: format!("onboard url for @{username}"),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::client::test_helpers::MockTelegramApi;

    fn mock_with_username(username: &str) -> MockTelegramApi {
        MockTelegramApi {
            response: serde_json::json!({
                "ok": true,
                "result": {
                    "id": 12345,
                    "is_bot": true,
                    "first_name": "Test Bot",
                    "username": username,
                }
            }),
        }
    }

    #[tokio::test]
    async fn builds_url_with_payload() {
        let client = mock_with_username("test_bot");
        let out = execute(
            &client,
            &serde_json::json!({ "payload": "springtale-onboard-formation-XYZ" }),
        )
        .await
        .unwrap();
        let url = out.output["url"].as_str().unwrap();
        assert_eq!(
            url,
            "https://t.me/test_bot?start=springtale-onboard-formation-XYZ"
        );
        assert_eq!(out.output["bot_username"].as_str().unwrap(), "test_bot");
    }

    #[tokio::test]
    async fn empty_payload_drops_query_string() {
        let client = mock_with_username("test_bot");
        let out = execute(&client, &serde_json::json!({})).await.unwrap();
        let url = out.output["url"].as_str().unwrap();
        assert_eq!(url, "https://t.me/test_bot");
    }

    #[tokio::test]
    async fn missing_username_errors_clearly() {
        let client = MockTelegramApi {
            response: serde_json::json!({ "ok": true, "result": {} }),
        };
        let err = execute(&client, &serde_json::json!({})).await.unwrap_err();
        match err {
            TelegramError::InvalidInput(msg) => {
                assert!(msg.contains("username"));
            }
            other => panic!("expected InvalidInput, got {other:?}"),
        }
    }
}
