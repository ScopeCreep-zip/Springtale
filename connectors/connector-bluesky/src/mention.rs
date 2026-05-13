//! Bluesky mention extractor — passive harvest is mostly a no-op
//! because AT Protocol bots post to their own account only;
//! there are no channels.
//!
//! However events that mention OTHER accounts (a mention received,
//! a follower added) DO carry the other account's DID — useful to
//! register so the bot can dispatch replies / mentions back.
//!
//! URI shape: `bluesky://account/{did}`.

use serde_json::Value;
use springtale_connector::mention::{HarvestedDestination, MentionExtractor};
use springtale_connector::workspace_key;

pub struct BlueskyMentionExtractor;

pub static BLUESKY_MENTION_EXTRACTOR: BlueskyMentionExtractor = BlueskyMentionExtractor;

impl MentionExtractor for BlueskyMentionExtractor {
    fn extract(&self, _trigger: &str, payload: &Value) -> Vec<HarvestedDestination> {
        // Common payload field names across Bluesky triggers.
        let did = payload
            .get("author_did")
            .or_else(|| payload.get("follower_did"))
            .or_else(|| payload.get("did"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        let Some(did) = did else {
            return Vec::new();
        };
        let handle = payload
            .get("author_handle")
            .or_else(|| payload.get("follower_handle"))
            .or_else(|| payload.get("handle"))
            .and_then(|v| v.as_str())
            .map(str::to_owned);

        let display_name = handle.clone().unwrap_or_else(|| did.clone());
        let mut metadata = serde_json::Map::new();
        metadata.insert("did".into(), Value::String(did.clone()));
        if let Some(h) = handle {
            metadata.insert("handle".into(), Value::String(h));
        }

        vec![HarvestedDestination {
            workspace_key: workspace_key::build("bluesky", &["account", &did]),
            display_name,
            kind: "account".into(),
            metadata: Value::Object(metadata),
        }]
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extract_from_mention_with_did_and_handle() {
        let ext = BlueskyMentionExtractor;
        let payload = json!({
            "author_did": "did:plc:abc123",
            "author_handle": "alice.bsky.social",
            "text": "hello",
        });
        let out = ext.extract("mention_received", &payload);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].workspace_key, "bluesky://account/did:plc:abc123");
        assert_eq!(out[0].display_name, "alice.bsky.social");
        assert_eq!(out[0].kind, "account");
    }

    #[test]
    fn extract_from_follower_added_uses_follower_did() {
        let ext = BlueskyMentionExtractor;
        let payload = json!({
            "follower_did": "did:plc:bob",
            "follower_handle": "bob.bsky.social",
        });
        let out = ext.extract("follower_added", &payload);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].workspace_key, "bluesky://account/did:plc:bob");
        assert_eq!(out[0].display_name, "bob.bsky.social");
    }

    #[test]
    fn extract_falls_back_to_did_when_no_handle() {
        let ext = BlueskyMentionExtractor;
        let payload = json!({ "did": "did:plc:nohandle" });
        let out = ext.extract("any", &payload);
        assert_eq!(out[0].display_name, "did:plc:nohandle");
    }

    #[test]
    fn extract_empty_when_no_did() {
        let ext = BlueskyMentionExtractor;
        let payload = json!({ "text": "nobody" });
        assert!(ext.extract("any", &payload).is_empty());
    }
}
