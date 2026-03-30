use springtale_connector::manifest::types::TriggerDecl;

pub fn trigger_declarations() -> Vec<TriggerDecl> {
    vec![message_received(), command_received()]
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_trigger_declarations_count() {
        assert_eq!(trigger_declarations().len(), 2);
    }

    #[test]
    fn test_trigger_names() {
        let triggers = trigger_declarations();
        assert_eq!(triggers[0].name, "message_received");
        assert_eq!(triggers[1].name, "command_received");
    }
}
