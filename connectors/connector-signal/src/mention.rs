//! Signal mention extractor — pulls group_id or sender phone from
//! event payloads.
//!
//! Signal connector triggers carry `source` (the E.164 phone of
//! the message sender) and optionally `group_id` when the message
//! is to a group. Direct messages have no `group_id`.
//!
//! URI shapes:
//!
//! ```text
//! signal://group/{group_id}
//! signal://user/{phone_e164}
//! ```

use serde_json::Value;
use springtale_connector::mention::{HarvestedDestination, MentionExtractor};
use springtale_connector::workspace_key;

pub struct SignalMentionExtractor;

pub static SIGNAL_MENTION_EXTRACTOR: SignalMentionExtractor = SignalMentionExtractor;

impl MentionExtractor for SignalMentionExtractor {
    fn extract(&self, _trigger: &str, payload: &Value) -> Vec<HarvestedDestination> {
        let group_id = payload
            .get("group_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty());
        if let Some(gid) = group_id {
            let mut metadata = serde_json::Map::new();
            metadata.insert("group_id".into(), Value::String(gid.to_owned()));
            return vec![HarvestedDestination {
                workspace_key: workspace_key::build("signal", &["group", gid]),
                display_name: format!("Signal group {gid}"),
                kind: "group".into(),
                metadata: Value::Object(metadata),
            }];
        }

        // No group → DM. Use the sender's phone (E.164) as the
        // workspace id. This is the destination we'd reply to.
        let source = payload
            .get("source")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty());
        let Some(phone) = source else {
            return Vec::new();
        };
        let mut metadata = serde_json::Map::new();
        metadata.insert("phone".into(), Value::String(phone.to_owned()));
        vec![HarvestedDestination {
            workspace_key: workspace_key::build("signal", &["user", phone]),
            display_name: phone.to_owned(),
            kind: "user".into(),
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
    fn extract_group_message() {
        let ext = SignalMentionExtractor;
        let payload = json!({
            "source": "+15551234",
            "group_id": "abcdef==",
            "timestamp": 1700000000,
            "text": "hi",
        });
        let out = ext.extract("message_received", &payload);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].workspace_key, "signal://group/abcdef==");
        assert_eq!(out[0].kind, "group");
    }

    #[test]
    fn extract_direct_message_falls_back_to_phone() {
        let ext = SignalMentionExtractor;
        let payload = json!({
            "source": "+15551234",
            "timestamp": 1700000000,
            "text": "hi",
        });
        let out = ext.extract("message_received", &payload);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].workspace_key, "signal://user/+15551234");
        assert_eq!(out[0].kind, "user");
    }

    #[test]
    fn extract_empty_group_id_falls_back_to_user() {
        let ext = SignalMentionExtractor;
        let payload = json!({
            "source": "+15551234",
            "group_id": "",
        });
        let out = ext.extract("message_received", &payload);
        assert_eq!(out[0].kind, "user");
    }

    #[test]
    fn extract_empty_when_no_source() {
        let ext = SignalMentionExtractor;
        let payload = json!({ "text": "hi" });
        assert!(ext.extract("message_received", &payload).is_empty());
    }
}
