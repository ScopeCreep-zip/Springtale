use springtale_connector::manifest::types::TriggerDecl;

pub fn trigger_declarations() -> Vec<TriggerDecl> {
    vec![
        message_received(),
        command_received(),
        user_joined(),
        user_parted(),
        topic_changed(),
    ]
}

fn message_received() -> TriggerDecl {
    TriggerDecl {
        name: "message_received".to_owned(),
        description: "Fires when a message is received in a joined channel or as a DM.".to_owned(),
        schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "nick": { "type": "string" },
                "target": { "type": "string", "description": "Channel name or bot nick (for DMs)" },
                "message": { "type": "string" },
                "host": { "type": "string" }
            },
            "required": ["nick", "target", "message"]
        })),
    }
}

fn command_received() -> TriggerDecl {
    TriggerDecl {
        name: "command_received".to_owned(),
        description:
            "Fires when a message starting with the command prefix (default: !) is received."
                .to_owned(),
        schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "nick": { "type": "string" },
                "target": { "type": "string" },
                "command": { "type": "string" },
                "args": { "type": "string" },
                "message": { "type": "string" }
            },
            "required": ["nick", "target", "command"]
        })),
    }
}

fn user_joined() -> TriggerDecl {
    TriggerDecl {
        name: "user_joined".to_owned(),
        description: "Fires when a user joins a channel.".to_owned(),
        schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "nick": { "type": "string" },
                "channel": { "type": "string" }
            },
            "required": ["nick", "channel"]
        })),
    }
}

fn user_parted() -> TriggerDecl {
    TriggerDecl {
        name: "user_parted".to_owned(),
        description: "Fires when a user leaves a channel.".to_owned(),
        schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "nick": { "type": "string" },
                "channel": { "type": "string" },
                "reason": { "type": "string" }
            },
            "required": ["nick", "channel"]
        })),
    }
}

fn topic_changed() -> TriggerDecl {
    TriggerDecl {
        name: "topic_changed".to_owned(),
        description: "Fires when a channel's topic is changed.".to_owned(),
        schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "channel": { "type": "string" },
                "topic": { "type": "string" },
                "nick": { "type": "string" }
            },
            "required": ["channel"]
        })),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_trigger_count() {
        assert_eq!(trigger_declarations().len(), 5);
    }

    #[test]
    fn test_trigger_names() {
        let triggers = trigger_declarations();
        assert_eq!(triggers[0].name, "message_received");
        assert_eq!(triggers[1].name, "command_received");
        assert_eq!(triggers[2].name, "user_joined");
        assert_eq!(triggers[3].name, "user_parted");
        assert_eq!(triggers[4].name, "topic_changed");
    }
}
