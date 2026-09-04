//! Telegram `discover_destinations` — the connector-side half of
//! the auto-onboard flow.
//!
//! Telegram bots can only know about chats that have previously
//! messaged them. We surface that constraint as a stateless
//! `getUpdates` call wrapped in the universal `discover_destinations`
//! action shape, so the runtime's onboarding stream can loop on it
//! every couple of seconds without owning any Telegram-specific
//! state.
//!
//! ## Input
//!
//! ```json
//! { "since_update_id": 42, "payload_filter": "springtale-onboard" }
//! ```
//!
//! Both fields optional. `since_update_id` is the offset returned by
//! the previous call's `next_update_id`. `payload_filter` keeps only
//! `/start <payload>` messages where the trailing argument matches —
//! lets the picker isolate the user's own `/start springtale-onboard`
//! from any other traffic the bot received.
//!
//! ## Output
//!
//! ```json
//! { "workspaces": [...], "next_update_id": 84 }
//! ```
//!
//! `workspaces` matches the shape every other connector's
//! `discover_destinations` returns. `next_update_id` is `max(update_id) + 1`
//! across the batch — the runtime stream uses this as the next call's
//! `since_update_id`. Telegram drops anything older than 24h server-side,
//! so the runtime never has to reason about update_id wraparound.
//!
//! ## Why not stream from inside the connector
//!
//! The action stays request/response so the runtime owns the polling
//! cadence + cancellation + event-emit. That keeps the connector
//! trait surface unchanged (we don't grow a streaming variant) and
//! lets connectors with one-shot REST enumeration (Discord, Slack)
//! reuse the exact same `discover_destinations` declaration without
//! taking on Telegram's stream lifecycle.

use std::collections::HashSet;

use serde_json::Value;
use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;
use springtale_connector::mention::MentionExtractor;

use crate::client::TelegramApi;
use crate::error::TelegramError;
use crate::mention::TELEGRAM_MENTION_EXTRACTOR;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        read_only: true,
        destructive: None,
        name: "discover_destinations".to_owned(),
        description:
            "Stateless poll of Telegram's getUpdates surface. Returns every chat that has \
             messaged this bot since `since_update_id` (optionally filtered to `/start <payload>` \
             messages), plus the next offset to feed back in. The runtime's onboard stream loops \
             on this every couple of seconds during the 60s onboarding window."
                .to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "since_update_id": {
                    "type": "integer",
                    "description": "Telegram `getUpdates` offset. Omit on the first call; pass `next_update_id` from the previous response thereafter."
                },
                "payload_filter": {
                    "type": "string",
                    "description": "Only return chats whose latest message is exactly `/start <payload_filter>` (or `/start@bot <payload_filter>` in groups)."
                }
            }
        })),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "workspaces": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "workspace_key": { "type": "string" },
                            "display_name":  { "type": "string" },
                            "kind":          { "type": "string" },
                            "metadata":      { "type": "object" }
                        }
                    }
                },
                "next_update_id": {
                    "type": "integer",
                    "description": "Max update_id + 1 across this batch. Pass back as `since_update_id` next call."
                }
            },
            "required": ["workspaces"]
        })),
    }
}

pub async fn execute(
    client: &dyn TelegramApi,
    input: &Value,
) -> Result<ActionResult, TelegramError> {
    let since = input.get("since_update_id").and_then(|v| v.as_i64());
    let payload_filter = input.get("payload_filter").and_then(|v| v.as_str());

    // `getUpdates(timeout=0)` is a true short-poll — Telegram returns
    // immediately with whatever's queued. The 60s onboarding window
    // owns the actual cadence by re-invoking us.
    let allowed = vec!["message".to_owned()];
    let result = client.get_updates(since, 0, &allowed).await?;
    let updates = result.as_array().cloned().unwrap_or_default();

    let mut rows = Vec::new();
    let mut seen: HashSet<i64> = HashSet::new();
    let mut max_update_id: Option<i64> = None;

    for update in &updates {
        if let Some(id) = update.get("update_id").and_then(|v| v.as_i64()) {
            max_update_id = Some(max_update_id.map_or(id, |m| m.max(id)));
        }

        let Some(message) = update.get("message") else {
            continue;
        };

        // Payload filter: match `/start <payload>` (1:1) OR
        // `/start@<bot_username> <payload>` (group/supergroup).
        // Telegram echoes the trailing argument verbatim — see
        // core.telegram.org/bots/features#deep-linking.
        if let Some(filter) = payload_filter {
            let text = message.get("text").and_then(|v| v.as_str()).unwrap_or("");
            if !is_start_payload_match(text, filter) {
                continue;
            }
        }

        // Reuse the mention extractor — the chat-→-workspace shaping
        // is already privacy-audited and tested. `extract` returns
        // 0 or 1 item per message; we dedupe by chat_id across the
        // batch because the same chat can fire multiple updates.
        let harvested = TELEGRAM_MENTION_EXTRACTOR.extract("message_received", message);
        for h in harvested {
            // Pull the chat id back out of the workspace key to dedupe.
            // Format is `telegram://chat/{id}`; strip the prefix and
            // parse. If the format ever changes the dedupe degrades
            // to "by key string" via the seen set below.
            if let Some(id_str) = h.workspace_key.rsplit('/').next()
                && let Ok(id) = id_str.parse::<i64>()
                && !seen.insert(id)
            {
                continue;
            }
            rows.push(serde_json::json!({
                "workspace_key": h.workspace_key,
                "display_name": h.display_name,
                "kind": h.kind,
                "metadata": h.metadata,
            }));
        }
    }

    let mut output = serde_json::Map::new();
    output.insert("workspaces".to_owned(), Value::Array(rows.clone()));
    if let Some(next) = max_update_id {
        output.insert("next_update_id".to_owned(), Value::from(next + 1));
    }

    let count = rows.len();
    Ok(ActionResult {
        success: true,
        output: Value::Object(output),
        message: format!("discovered {count} destination(s)"),
    })
}

/// `/start <payload>` or `/start@<bot> <payload>` — exact match on
/// the trailing token. Whitespace-tolerant after the command.
fn is_start_payload_match(text: &str, payload: &str) -> bool {
    let mut parts = text.split_whitespace();
    let Some(cmd) = parts.next() else {
        return false;
    };
    let is_start = cmd == "/start" || cmd.starts_with("/start@");
    if !is_start {
        return false;
    }
    parts.next() == Some(payload)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::client::test_helpers::MockTelegramApi;

    /// The real `TelegramClient.get_updates` runs the response through
    /// `handle_telegram_response`, which unwraps `{ok, result}` and
    /// returns the inner `result` value directly. Mirror that — the
    /// mock surfaces the unwrapped array straight back to the action.
    fn mock_with_updates(updates: serde_json::Value) -> MockTelegramApi {
        MockTelegramApi { response: updates }
    }

    #[test]
    fn declaration_name_and_required_outputs() {
        let d = declaration();
        assert_eq!(d.name, "discover_destinations");
        let out = d.output_schema.unwrap();
        let req = out.get("required").and_then(|v| v.as_array()).unwrap();
        assert!(req.iter().any(|v| v == "workspaces"));
    }

    #[test]
    fn is_start_payload_match_accepts_exact_dm_form() {
        assert!(is_start_payload_match("/start hello", "hello"));
    }

    #[test]
    fn is_start_payload_match_accepts_group_mention_form() {
        assert!(is_start_payload_match("/start@my_bot hello", "hello"));
    }

    #[test]
    fn is_start_payload_match_rejects_mismatched_payload() {
        assert!(!is_start_payload_match("/start hello", "world"));
    }

    #[test]
    fn is_start_payload_match_rejects_non_start_command() {
        assert!(!is_start_payload_match("/help hello", "hello"));
    }

    #[tokio::test]
    async fn extracts_chat_from_start_message_when_filter_matches() {
        let client = mock_with_updates(serde_json::json!([
            {
                "update_id": 100,
                "message": {
                    "message_id": 1,
                    "chat": {
                        "id": 12345,
                        "type": "private",
                        "first_name": "Alice",
                        "username": "alicebsky"
                    },
                    "from": { "id": 99, "is_bot": false, "first_name": "Alice" },
                    "text": "/start springtale-onboard",
                    "date": 1700000000
                }
            }
        ]));

        let result = execute(
            &client,
            &serde_json::json!({ "payload_filter": "springtale-onboard" }),
        )
        .await
        .unwrap();

        let arr = result.output["workspaces"].as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["workspace_key"], "telegram://chat/12345");
        assert_eq!(arr[0]["display_name"], "@alicebsky");
        assert_eq!(result.output["next_update_id"].as_i64().unwrap(), 101);
    }

    #[tokio::test]
    async fn filters_out_unrelated_start_payloads() {
        let client = mock_with_updates(serde_json::json!([
            {
                "update_id": 1,
                "message": {
                    "chat": { "id": 1, "type": "private", "first_name": "Bob" },
                    "text": "/start someone-elses-payload",
                    "date": 1
                }
            },
            {
                "update_id": 2,
                "message": {
                    "chat": { "id": 2, "type": "private", "first_name": "Cara" },
                    "text": "/start springtale-onboard",
                    "date": 2
                }
            }
        ]));

        let result = execute(
            &client,
            &serde_json::json!({ "payload_filter": "springtale-onboard" }),
        )
        .await
        .unwrap();

        let arr = result.output["workspaces"].as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["workspace_key"], "telegram://chat/2");
    }

    #[tokio::test]
    async fn returns_all_chats_when_no_filter_supplied() {
        let client = mock_with_updates(serde_json::json!([
            {
                "update_id": 10,
                "message": {
                    "chat": { "id": 1, "type": "private", "first_name": "A" },
                    "text": "hi",
                    "date": 1
                }
            },
            {
                "update_id": 11,
                "message": {
                    "chat": { "id": 2, "type": "supergroup", "title": "Group" },
                    "text": "yo",
                    "date": 2
                }
            }
        ]));

        let result = execute(&client, &serde_json::json!({})).await.unwrap();
        let arr = result.output["workspaces"].as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(result.output["next_update_id"].as_i64().unwrap(), 12);
    }

    #[tokio::test]
    async fn dedupes_same_chat_across_multiple_updates() {
        let client = mock_with_updates(serde_json::json!([
            {
                "update_id": 1,
                "message": {
                    "chat": { "id": 7, "type": "private", "first_name": "Z" },
                    "text": "/start springtale-onboard",
                    "date": 1
                }
            },
            {
                "update_id": 2,
                "message": {
                    "chat": { "id": 7, "type": "private", "first_name": "Z" },
                    "text": "/start springtale-onboard",
                    "date": 2
                }
            }
        ]));

        let result = execute(
            &client,
            &serde_json::json!({ "payload_filter": "springtale-onboard" }),
        )
        .await
        .unwrap();

        let arr = result.output["workspaces"].as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(result.output["next_update_id"].as_i64().unwrap(), 3);
    }

    #[tokio::test]
    async fn no_updates_returns_empty_no_next_update_id() {
        let client = mock_with_updates(serde_json::json!([]));
        let result = execute(&client, &serde_json::json!({})).await.unwrap();
        let arr = result.output["workspaces"].as_array().unwrap();
        assert!(arr.is_empty());
        assert!(result.output.get("next_update_id").is_none());
    }
}
