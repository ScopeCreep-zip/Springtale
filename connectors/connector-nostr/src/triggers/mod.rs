use springtale_connector::manifest::types::TriggerDecl;

pub fn trigger_declarations() -> Vec<TriggerDecl> {
    vec![
        note_received(),
        dm_received(),
        mention_received(),
        reaction_received(),
    ]
}

fn note_received() -> TriggerDecl {
    TriggerDecl {
        name: "note_received".to_owned(),
        description: "Fires when a text note (kind 1) is received from subscribed relays."
            .to_owned(),
        schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "event_id": { "type": "string" },
                "pubkey": { "type": "string", "description": "Author's public key (hex)" },
                "content": { "type": "string" },
                "created_at": { "type": "integer" },
                "relay_url": { "type": "string" }
            },
            "required": ["event_id", "pubkey", "content", "created_at"]
        })),
    }
}

fn dm_received() -> TriggerDecl {
    TriggerDecl {
        name: "dm_received".to_owned(),
        description:
            "Fires when an encrypted DM is received and decrypted (NIP-44 via NIP-17 gift wrap)."
                .to_owned(),
        schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "sender_pubkey": { "type": "string" },
                "content": { "type": "string", "description": "Decrypted message content" },
                "created_at": { "type": "integer" },
                "relay_url": { "type": "string" }
            },
            "required": ["sender_pubkey", "content", "created_at"]
        })),
    }
}

fn mention_received() -> TriggerDecl {
    TriggerDecl {
        name: "mention_received".to_owned(),
        description: "Fires when the bot's public key is mentioned in a note (p-tag).".to_owned(),
        schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "event_id": { "type": "string" },
                "pubkey": { "type": "string" },
                "content": { "type": "string" },
                "created_at": { "type": "integer" }
            },
            "required": ["event_id", "pubkey", "content"]
        })),
    }
}

fn reaction_received() -> TriggerDecl {
    TriggerDecl {
        name: "reaction_received".to_owned(),
        description: "Fires when a reaction (kind 7) to the bot's events is received.".to_owned(),
        schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "event_id": { "type": "string" },
                "pubkey": { "type": "string" },
                "content": { "type": "string", "description": "Reaction emoji (e.g., '+')" },
                "target_event_id": { "type": "string" }
            },
            "required": ["event_id", "pubkey", "content"]
        })),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_trigger_declarations_count() {
        assert_eq!(trigger_declarations().len(), 4);
    }

    #[test]
    fn test_trigger_names() {
        let triggers = trigger_declarations();
        assert_eq!(triggers[0].name, "note_received");
        assert_eq!(triggers[1].name, "dm_received");
        assert_eq!(triggers[2].name, "mention_received");
        assert_eq!(triggers[3].name, "reaction_received");
    }
}
