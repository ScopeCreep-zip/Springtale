use springtale_connector::manifest::types::TriggerDecl;

pub fn trigger_declarations() -> Vec<TriggerDecl> {
    vec![
        message_received(),
        group_message_received(),
        disappearing_timer_changed(),
    ]
}

fn message_received() -> TriggerDecl {
    TriggerDecl {
        name: "message_received".to_owned(),
        description: "Fires when a 1:1 Signal message is received. \
                      E2E encrypted — signal-cli decrypts locally."
            .to_owned(),
        schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "source": { "type": "string", "description": "Sender phone number or UUID" },
                "message": { "type": "string" },
                "timestamp": { "type": "integer" },
                "expires_in_seconds": { "type": "integer" }
            },
            "required": ["source", "timestamp"]
        })),
    }
}

fn group_message_received() -> TriggerDecl {
    TriggerDecl {
        name: "group_message_received".to_owned(),
        description: "Fires when a group message is received.".to_owned(),
        schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "source": { "type": "string" },
                "group_id": { "type": "string" },
                "message": { "type": "string" },
                "timestamp": { "type": "integer" }
            },
            "required": ["source", "group_id", "timestamp"]
        })),
    }
}

fn disappearing_timer_changed() -> TriggerDecl {
    TriggerDecl {
        name: "disappearing_timer_changed".to_owned(),
        description: "Fires when the disappearing message timer changes for a conversation."
            .to_owned(),
        schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "source": { "type": "string" },
                "expires_in_seconds": { "type": "integer" }
            },
            "required": ["source", "expires_in_seconds"]
        })),
    }
}
