//! Nostr mention extractor — pulls pubkeys from inbound events
//! into the formation's external-workspace directory.
//!
//! Triggers carry `pubkey` (author of a note / DM / reaction) or
//! `sender_pubkey` (DM-specific field). We treat every harvested
//! pubkey as a potential DM target — Nostr's NIP-04 / NIP-17
//! encrypted DMs are addressable by pubkey.
//!
//! URI shape: `nostr://pubkey/{hex}`. Bluesky and Nostr both
//! flatten to a single "account"-shaped scheme since there are no
//! channels or groups at the protocol level — destinations are
//! identities.

use serde_json::Value;
use springtale_connector::mention::{HarvestedDestination, MentionExtractor};
use springtale_connector::workspace_key;

pub struct NostrMentionExtractor;

pub static NOSTR_MENTION_EXTRACTOR: NostrMentionExtractor = NostrMentionExtractor;

impl MentionExtractor for NostrMentionExtractor {
    fn extract(&self, _trigger: &str, payload: &Value) -> Vec<HarvestedDestination> {
        let pubkey = payload
            .get("sender_pubkey")
            .or_else(|| payload.get("pubkey"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        let Some(pubkey) = pubkey else {
            return Vec::new();
        };

        let mut metadata = serde_json::Map::new();
        metadata.insert("pubkey".into(), Value::String(pubkey.clone()));

        vec![HarvestedDestination {
            workspace_key: workspace_key::build("nostr", &["pubkey", &pubkey]),
            // Display: first 8 + last 4 chars of the hex. NIP-19
            // npub bech32 encoding is the polite form, but we
            // don't pull in the bech32 crate just for displays —
            // the picker can do it client-side if needed.
            display_name: short_pubkey_label(&pubkey),
            kind: "user".into(),
            metadata: Value::Object(metadata),
        }]
    }
}

fn short_pubkey_label(pubkey: &str) -> String {
    if pubkey.len() <= 12 {
        return pubkey.to_owned();
    }
    let prefix: String = pubkey.chars().take(8).collect();
    let suffix: String = pubkey.chars().rev().take(4).collect::<String>().chars().rev().collect();
    format!("{prefix}…{suffix}")
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extract_from_note_received() {
        let ext = NostrMentionExtractor;
        let payload = json!({
            "event_id": "e1",
            "pubkey": "deadbeef0102030405060708090a0b0c0d0e0f10",
            "content": "hi",
            "created_at": 1700000000,
        });
        let out = ext.extract("note_received", &payload);
        assert_eq!(out.len(), 1);
        assert_eq!(
            out[0].workspace_key,
            "nostr://pubkey/deadbeef0102030405060708090a0b0c0d0e0f10"
        );
        assert_eq!(out[0].kind, "user");
        // 8 prefix + … + 4 suffix
        assert_eq!(out[0].display_name, "deadbeef…0f10");
    }

    #[test]
    fn extract_from_dm_uses_sender_pubkey() {
        let ext = NostrMentionExtractor;
        let payload = json!({
            "sender_pubkey": "feedfacedeadbeef",
            "content": "encrypted",
            "created_at": 1,
        });
        let out = ext.extract("encrypted_dm_received", &payload);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].workspace_key, "nostr://pubkey/feedfacedeadbeef");
    }

    #[test]
    fn short_pubkey_label_handles_short_inputs() {
        assert_eq!(short_pubkey_label("abc"), "abc");
    }

    #[test]
    fn extract_empty_when_no_pubkey() {
        let ext = NostrMentionExtractor;
        let payload = json!({ "content": "no pubkey here" });
        assert!(ext.extract("note_received", &payload).is_empty());
    }
}
