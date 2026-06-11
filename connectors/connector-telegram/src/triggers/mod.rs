use springtale_connector::manifest::types::TriggerDecl;

pub mod normalize;

pub fn trigger_declarations() -> Vec<TriggerDecl> {
    vec![
        message_received(),
        command_received(),
        callback_query_received(),
    ]
}

fn message_received() -> TriggerDecl {
    TriggerDecl {
        name: "message_received".to_owned(),
        description: "Fires when any message is received in a chat the bot can see.".to_owned(),
        schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "message_id": { "type": "integer" },
                "chat": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "integer" },
                        "type": { "type": "string", "enum": ["private", "group", "supergroup", "channel"] }
                    },
                    "required": ["id", "type"]
                },
                "from": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "integer" },
                        "is_bot": { "type": "boolean" },
                        "first_name": { "type": "string" },
                        "username": { "type": "string" }
                    },
                    "required": ["id", "is_bot", "first_name"]
                },
                "text": { "type": "string" },
                "date": { "type": "integer" }
            },
            "required": ["message_id", "chat", "date"]
        })),
    }
}

fn command_received() -> TriggerDecl {
    TriggerDecl {
        name: "command_received".to_owned(),
        description: "Fires when a bot command (/command) is received.".to_owned(),
        schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "message_id": { "type": "integer" },
                "chat": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "integer" },
                        "type": { "type": "string" }
                    },
                    "required": ["id", "type"]
                },
                "from": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "integer" },
                        "is_bot": { "type": "boolean" },
                        "first_name": { "type": "string" },
                        "username": { "type": "string" }
                    },
                    "required": ["id", "is_bot", "first_name"]
                },
                "command": { "type": "string" },
                "args": { "type": "string" },
                "text": { "type": "string" },
                "date": { "type": "integer" }
            },
            "required": ["message_id", "chat", "command", "date"]
        })),
    }
}

/// Fires when a user taps an inline keyboard button.
///
/// Telegram delivers these as `callback_query` updates (separate from `message`).
/// The payload includes the original `callback_data` string plus the source user
/// and message. Handlers should call `answer_callback_query` within 10 seconds to
/// dismiss the loading spinner on the button.
fn callback_query_received() -> TriggerDecl {
    TriggerDecl {
        name: "callback_query_received".to_owned(),
        description: "Fires when a user taps an inline keyboard button (Telegram callback_query)."
            .to_owned(),
        schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "id": { "type": "string", "description": "Callback query ID (needed to answer)." },
                "from": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "integer" },
                        "is_bot": { "type": "boolean" },
                        "first_name": { "type": "string" },
                        "username": { "type": "string" }
                    },
                    "required": ["id", "is_bot", "first_name"]
                },
                "message": {
                    "type": "object",
                    "description": "The message the inline keyboard was attached to."
                },
                "data": {
                    "type": "string",
                    "description": "The callback_data string from the pressed button."
                }
            },
            "required": ["id", "from"]
        })),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_trigger_declarations_count() {
        assert_eq!(trigger_declarations().len(), 3);
    }

    #[test]
    fn test_trigger_names() {
        let triggers = trigger_declarations();
        assert_eq!(triggers[0].name, "message_received");
        assert_eq!(triggers[1].name, "command_received");
        assert_eq!(triggers[2].name, "callback_query_received");
    }
}
