use springtale_connector::manifest::types::TriggerDecl;

pub fn trigger_declarations() -> Vec<TriggerDecl> {
    vec![
        interaction_received(),
        message_received(),
        dm_received(),
        reaction_added(),
        member_joined(),
    ]
}

fn interaction_received() -> TriggerDecl {
    TriggerDecl {
        name: "interaction_received".to_owned(),
        description: "Fires when a slash command or other interaction is invoked. \
                      Always available — does not require MESSAGE_CONTENT intent."
            .to_owned(),
        schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "interaction_id": { "type": "string" },
                "command_name": { "type": "string" },
                "user_id": { "type": "string" },
                "channel_id": { "type": "string" },
                "guild_id": { "type": "string" },
                "options": { "type": "object" }
            },
            "required": ["interaction_id", "command_name", "user_id", "channel_id"]
        })),
    }
}

fn message_received() -> TriggerDecl {
    TriggerDecl {
        name: "message_received".to_owned(),
        description: "Fires when a message is sent in a guild channel. \
                      Requires enable_message_content=true (privileged MESSAGE_CONTENT intent). \
                      WARNING: This allows the bot to read ALL messages in ALL channels."
            .to_owned(),
        schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "message_id": { "type": "string" },
                "channel_id": { "type": "string" },
                "guild_id": { "type": "string" },
                "user_id": { "type": "string" },
                "content": { "type": "string" },
                "timestamp": { "type": "string" }
            },
            "required": ["message_id", "channel_id", "user_id"]
        })),
    }
}

fn dm_received() -> TriggerDecl {
    TriggerDecl {
        name: "dm_received".to_owned(),
        description: "Fires when a direct message is received. \
                      Requires enable_direct_messages=true."
            .to_owned(),
        schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "message_id": { "type": "string" },
                "channel_id": { "type": "string" },
                "user_id": { "type": "string" },
                "content": { "type": "string" },
                "timestamp": { "type": "string" }
            },
            "required": ["message_id", "channel_id", "user_id"]
        })),
    }
}

fn reaction_added() -> TriggerDecl {
    TriggerDecl {
        name: "reaction_added".to_owned(),
        description: "Fires when a reaction is added to a message. \
                      Requires enable_reactions=true."
            .to_owned(),
        schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "user_id": { "type": "string" },
                "channel_id": { "type": "string" },
                "message_id": { "type": "string" },
                "guild_id": { "type": "string" },
                "emoji": { "type": "string" }
            },
            "required": ["user_id", "channel_id", "message_id", "emoji"]
        })),
    }
}

fn member_joined() -> TriggerDecl {
    TriggerDecl {
        name: "member_joined".to_owned(),
        description: "Fires when a user joins a guild. \
                      Requires GUILD_MEMBERS privileged intent (not enabled by default)."
            .to_owned(),
        schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "user_id": { "type": "string" },
                "guild_id": { "type": "string" },
                "joined_at": { "type": "string" }
            },
            "required": ["user_id", "guild_id"]
        })),
    }
}
