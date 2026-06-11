//! Anti-corruption normalization for Telegram events.
//!
//! Both polling and webhook deliver a raw Telegram `Update` whose message
//! is deeply nested (`message.chat.id`, `message.from.id`, …). Recipes
//! consume FLAT fields (`${trigger.chat_id}`, `${trigger.text}`,
//! `${trigger.command}`, `${trigger.args}`). This is the boundary that
//! maps the raw Update to that flat shape, so a Telegram recipe resolves
//! to a real chat id / text instead of a nested object or a literal
//! `${trigger.chat_id}` placeholder.

use serde_json::{Map, Value};

/// Map a raw Telegram `Update` (or a bare message/callback object) into
/// the flat trigger schema recipes consume.
pub fn normalize(trigger: &str, raw: &Value) -> Value {
    // Callback-query updates carry a different shape.
    if trigger == "callback_query_received" || raw.get("callback_query").is_some() {
        let cq = raw.get("callback_query").unwrap_or(raw);
        let mut out = Map::new();
        insert_if(&mut out, "id", cq.get("id").cloned());
        insert_if(&mut out, "user_id", cq.pointer("/from/id").cloned());
        insert_if(&mut out, "chat_id", cq.pointer("/message/chat/id").cloned());
        insert_if(&mut out, "data", cq.get("data").cloned());
        return Value::Object(out);
    }

    // A raw Update wraps the message; tolerate being handed the message
    // object directly (e.g. an already-unwrapped payload).
    let msg = raw
        .get("message")
        .or_else(|| raw.get("edited_message"))
        .or_else(|| raw.get("channel_post"))
        .unwrap_or(raw);

    let mut out = Map::new();
    insert_if(&mut out, "message_id", msg.get("message_id").cloned());
    insert_if(&mut out, "chat_id", msg.pointer("/chat/id").cloned());
    insert_if(&mut out, "chat_type", msg.pointer("/chat/type").cloned());
    insert_if(&mut out, "user_id", msg.pointer("/from/id").cloned());
    insert_if(&mut out, "username", msg.pointer("/from/username").cloned());
    insert_if(&mut out, "date", msg.get("date").cloned());

    let text = msg.get("text").and_then(Value::as_str).unwrap_or("");
    if !text.is_empty() {
        out.insert("text".to_owned(), Value::String(text.to_owned()));
    }
    if text.starts_with('/') {
        let (command, args) = crate::webhook::parse_command(text);
        out.insert("command".to_owned(), Value::String(command));
        out.insert("args".to_owned(), Value::String(args));
    }

    Value::Object(out)
}

fn insert_if(out: &mut Map<String, Value>, key: &str, val: Option<Value>) {
    match val {
        Some(v) if !v.is_null() => {
            out.insert(key.to_owned(), v);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Real Telegram getUpdates / webhook Update shape for a text message.
    #[test]
    fn normalizes_text_message() {
        let raw = json!({
            "update_id": 100,
            "message": {
                "message_id": 5,
                "from": { "id": 99, "is_bot": false, "first_name": "Kali", "username": "kali" },
                "chat": { "id": 4242, "type": "private" },
                "text": "hello world",
                "date": 1700000000
            }
        });
        let flat = normalize("message", &raw);
        assert_eq!(flat["chat_id"], 4242); // flat integer, not a nested object
        assert_eq!(flat["text"], "hello world");
        assert_eq!(flat["message_id"], 5);
        assert_eq!(flat["user_id"], 99);
        assert_eq!(flat["username"], "kali");
        assert!(flat.get("command").is_none());
    }

    #[test]
    fn normalizes_command_message() {
        let raw = json!({
            "update_id": 101,
            "message": {
                "message_id": 6,
                "from": { "id": 99, "is_bot": false, "first_name": "Kali" },
                "chat": { "id": 4242, "type": "group" },
                "text": "/broadcast hello team",
                "date": 1700000001
            }
        });
        let flat = normalize("command_received", &raw);
        assert_eq!(flat["chat_id"], 4242);
        assert_eq!(flat["command"], "broadcast");
        assert_eq!(flat["args"], "hello team");
        assert_eq!(flat["text"], "/broadcast hello team");
    }

    #[test]
    fn normalizes_callback_query() {
        let raw = json!({
            "update_id": 102,
            "callback_query": {
                "id": "cbq-1",
                "from": { "id": 99 },
                "message": { "chat": { "id": 4242 } },
                "data": "btn_yes"
            }
        });
        let flat = normalize("callback_query_received", &raw);
        assert_eq!(flat["id"], "cbq-1");
        assert_eq!(flat["chat_id"], 4242);
        assert_eq!(flat["data"], "btn_yes");
    }

    #[test]
    fn no_nested_objects_leak() {
        let raw = json!({ "message": { "message_id": 1, "chat": { "id": 7 }, "date": 1 } });
        let flat = normalize("message", &raw);
        for (_, v) in flat.as_object().unwrap() {
            assert!(
                !v.is_object() && !v.is_array(),
                "flat field leaked a nested value: {v}"
            );
        }
    }
}
