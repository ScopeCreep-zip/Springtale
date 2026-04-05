use springtale_connector::manifest::types::TriggerDecl;

pub fn trigger_declarations() -> Vec<TriggerDecl> {
    vec![
        slash_command(),
        message_received(),
        app_mentioned(),
        reaction_added(),
        thread_reply(),
    ]
}

fn slash_command() -> TriggerDecl {
    TriggerDecl {
        name: "slash_command".to_owned(),
        description: "Fires when a slash command is invoked. \
                      Received via Socket Mode — no public URL needed."
            .to_owned(),
        schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "The command name (e.g., /remind)" },
                "text": { "type": "string", "description": "Arguments after the command" },
                "user_id": { "type": "string" },
                "channel_id": { "type": "string" },
                "response_url": { "type": "string" }
            },
            "required": ["command", "user_id", "channel_id"]
        })),
    }
}

fn message_received() -> TriggerDecl {
    TriggerDecl {
        name: "message_received".to_owned(),
        description: "Fires when a message is posted in a channel the bot is in. \
                      WARNING: All messages are visible to workspace admins."
            .to_owned(),
        schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "user_id": { "type": "string" },
                "channel_id": { "type": "string" },
                "text": { "type": "string" },
                "ts": { "type": "string", "description": "Message timestamp (unique ID)" }
            },
            "required": ["user_id", "channel_id", "ts"]
        })),
    }
}

fn app_mentioned() -> TriggerDecl {
    TriggerDecl {
        name: "app_mentioned".to_owned(),
        description: "Fires when the bot is @mentioned in a channel.".to_owned(),
        schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "user_id": { "type": "string" },
                "channel_id": { "type": "string" },
                "text": { "type": "string" },
                "ts": { "type": "string" }
            },
            "required": ["user_id", "channel_id", "ts"]
        })),
    }
}

fn reaction_added() -> TriggerDecl {
    TriggerDecl {
        name: "reaction_added".to_owned(),
        description: "Fires when a reaction emoji is added to a message.".to_owned(),
        schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "user_id": { "type": "string" },
                "reaction": { "type": "string", "description": "Emoji name (without colons)" },
                "item_channel": { "type": "string" },
                "item_ts": { "type": "string" }
            },
            "required": ["user_id", "reaction"]
        })),
    }
}

fn thread_reply() -> TriggerDecl {
    TriggerDecl {
        name: "thread_reply".to_owned(),
        description: "Fires when a reply is posted in a thread.".to_owned(),
        schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "user_id": { "type": "string" },
                "channel_id": { "type": "string" },
                "text": { "type": "string" },
                "ts": { "type": "string" },
                "thread_ts": { "type": "string", "description": "Parent message timestamp" }
            },
            "required": ["user_id", "channel_id", "ts", "thread_ts"]
        })),
    }
}
