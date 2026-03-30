use springtale_connector::manifest::types::TriggerDecl;

/// All trigger declarations for the Kick connector.
///
/// Schemas match the actual Kick webhook event payloads (researched against
/// KickEngineering/KickDevDocs). Kick sends User objects (not flat strings)
/// and a single `livestream.status.updated` event with `is_live` boolean
/// (not separate live/offline events).
pub fn trigger_declarations() -> Vec<TriggerDecl> {
    vec![
        chat_message(),
        stream_live(),
        stream_offline(),
        channel_followed(),
    ]
}

/// Kick User object schema, shared across event payloads.
fn user_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "user_id": { "type": "integer" },
            "username": { "type": "string" },
            "is_verified": { "type": "boolean" },
            "profile_picture": { "type": "string" },
            "channel_slug": { "type": "string" }
        }
    })
}

/// Kick event: `chat.message.sent`
fn chat_message() -> TriggerDecl {
    TriggerDecl {
        name: "chat_message".to_owned(),
        description: "Fires when a chat message is sent in a subscribed channel.".to_owned(),
        schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "message_id": { "type": "string", "description": "Unique message ID." },
                "content": { "type": "string", "description": "Message content." },
                "sender": user_schema(),
                "broadcaster": user_schema(),
                "emotes": {
                    "type": "array",
                    "items": { "type": "object" },
                    "description": "Emotes used in the message."
                },
                "created_at": { "type": "string", "description": "ISO 8601 timestamp." }
            },
            "required": ["message_id", "content", "sender", "broadcaster"]
        })),
    }
}

/// Kick event: `livestream.status.updated` with `is_live: true`
fn stream_live() -> TriggerDecl {
    TriggerDecl {
        name: "stream_live".to_owned(),
        description: "Fires when a subscribed channel goes live (is_live=true).".to_owned(),
        schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "broadcaster": user_schema(),
                "is_live": { "type": "boolean", "description": "Always true for this trigger." },
                "title": { "type": "string", "description": "Stream title." },
                "started_at": { "type": "string", "description": "ISO 8601 timestamp." }
            },
            "required": ["broadcaster", "is_live"]
        })),
    }
}

/// Kick event: `livestream.status.updated` with `is_live: false`
fn stream_offline() -> TriggerDecl {
    TriggerDecl {
        name: "stream_offline".to_owned(),
        description: "Fires when a subscribed channel goes offline (is_live=false).".to_owned(),
        schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "broadcaster": user_schema(),
                "is_live": { "type": "boolean", "description": "Always false for this trigger." },
                "title": { "type": "string", "description": "Stream title." },
                "ended_at": { "type": "string", "description": "ISO 8601 timestamp." }
            },
            "required": ["broadcaster", "is_live"]
        })),
    }
}

/// Kick event: `channel.followed`
fn channel_followed() -> TriggerDecl {
    TriggerDecl {
        name: "channel_followed".to_owned(),
        description: "Fires when someone follows a subscribed channel.".to_owned(),
        schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "broadcaster": user_schema(),
                "follower": user_schema()
            },
            "required": ["broadcaster", "follower"]
        })),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_trigger_count() {
        assert_eq!(trigger_declarations().len(), 4);
    }

    #[test]
    fn test_trigger_names() {
        let triggers = trigger_declarations();
        let names: Vec<&str> = triggers.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"chat_message"));
        assert!(names.contains(&"stream_live"));
        assert!(names.contains(&"stream_offline"));
        assert!(names.contains(&"channel_followed"));
    }

    #[test]
    fn test_all_triggers_have_schemas() {
        for trigger in trigger_declarations() {
            assert!(
                trigger.schema.is_some(),
                "trigger {} missing schema",
                trigger.name
            );
        }
    }

    #[test]
    fn test_chat_message_sender_is_object() {
        let trigger = chat_message();
        let schema = trigger.schema.unwrap();
        let sender_type = schema["properties"]["sender"]["type"].as_str().unwrap();
        assert_eq!(
            sender_type, "object",
            "sender must be a User object, not a string"
        );
    }

    #[test]
    fn test_stream_live_has_is_live_field() {
        let trigger = stream_live();
        let schema = trigger.schema.unwrap();
        assert!(schema["properties"]["is_live"].is_object());
    }
}
