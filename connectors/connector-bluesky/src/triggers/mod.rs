use springtale_connector::manifest::types::TriggerDecl;

/// All trigger declarations for the Bluesky connector.
///
/// Schemas match the actual ATProto record structures from Jetstream events.
/// Researched against bluesky-social/atproto lexicon definitions.
pub fn trigger_declarations() -> Vec<TriggerDecl> {
    vec![mention(), follow(), like(), repost()]
}

/// Jetstream commit event for `app.bsky.feed.post` where the post
/// mentions the authenticated user (checked via `post_mentions_did()`
/// in the firehose module).
fn mention() -> TriggerDecl {
    TriggerDecl {
        name: "mention".to_owned(),
        description: "Fires when the authenticated user is mentioned in a post.".to_owned(),
        schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "did": { "type": "string", "description": "DID of the post author." },
                "uri": { "type": "string", "description": "AT URI of the post." },
                "cid": { "type": "string", "description": "CID of the post." },
                "text": { "type": "string", "description": "Post text." },
                "facets": {
                    "type": "array",
                    "description": "Rich text facets (mentions, links, tags).",
                    "items": {
                        "type": "object",
                        "properties": {
                            "features": { "type": "array" },
                            "index": { "type": "object" }
                        }
                    }
                },
                "createdAt": { "type": "string", "description": "ISO 8601 timestamp." }
            },
            "required": ["did", "uri", "text"]
        })),
    }
}

/// Jetstream commit event for `app.bsky.graph.follow`.
fn follow() -> TriggerDecl {
    TriggerDecl {
        name: "follow".to_owned(),
        description: "Fires when someone follows the authenticated user.".to_owned(),
        schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "did": { "type": "string", "description": "DID of the follower." },
                "uri": { "type": "string", "description": "AT URI of the follow record." },
                "subject": { "type": "string", "description": "DID of the account being followed." },
                "createdAt": { "type": "string", "description": "ISO 8601 timestamp." }
            },
            "required": ["did", "uri", "subject"]
        })),
    }
}

/// Jetstream commit event for `app.bsky.feed.like`.
fn like() -> TriggerDecl {
    TriggerDecl {
        name: "like".to_owned(),
        description: "Fires when someone likes one of the authenticated user's posts.".to_owned(),
        schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "did": { "type": "string", "description": "DID of the person who liked." },
                "subject": {
                    "type": "object",
                    "description": "The liked post reference.",
                    "properties": {
                        "uri": { "type": "string", "description": "AT URI of the liked post." },
                        "cid": { "type": "string", "description": "CID of the liked post." }
                    },
                    "required": ["uri", "cid"]
                },
                "createdAt": { "type": "string", "description": "ISO 8601 timestamp." }
            },
            "required": ["did", "subject"]
        })),
    }
}

/// Jetstream commit event for `app.bsky.feed.repost`.
fn repost() -> TriggerDecl {
    TriggerDecl {
        name: "repost".to_owned(),
        description: "Fires when someone reposts one of the authenticated user's posts.".to_owned(),
        schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "did": { "type": "string", "description": "DID of the person who reposted." },
                "subject": {
                    "type": "object",
                    "description": "The reposted post reference.",
                    "properties": {
                        "uri": { "type": "string", "description": "AT URI of the reposted post." },
                        "cid": { "type": "string", "description": "CID of the reposted post." }
                    },
                    "required": ["uri", "cid"]
                },
                "createdAt": { "type": "string", "description": "ISO 8601 timestamp." }
            },
            "required": ["did", "subject"]
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
        assert!(names.contains(&"mention"));
        assert!(names.contains(&"follow"));
        assert!(names.contains(&"like"));
        assert!(names.contains(&"repost"));
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
}
