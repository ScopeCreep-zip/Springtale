/// Jetstream event types for filtering.
///
/// These correspond to ATProto record collections that can be filtered
/// via the `wantedCollections` query parameter on the Jetstream WebSocket.
pub const COLLECTION_POST: &str = "app.bsky.feed.post";
pub const COLLECTION_LIKE: &str = "app.bsky.feed.like";
pub const COLLECTION_REPOST: &str = "app.bsky.feed.repost";
pub const COLLECTION_FOLLOW: &str = "app.bsky.graph.follow";

/// Jetstream subscribe URL with collection filters.
///
/// Format: `wss://jetstream2.us-west.bsky.network/subscribe?wantedCollections=app.bsky.feed.post&wantedCollections=...`
pub fn build_jetstream_url(base_url: &str, collections: &[&str], cursor: Option<u64>) -> String {
    let mut url = base_url.to_owned();
    let mut first = true;

    for collection in collections {
        if first {
            url.push('?');
            first = false;
        } else {
            url.push('&');
        }
        url.push_str("wantedCollections=");
        url.push_str(collection);
    }

    if let Some(cursor_val) = cursor {
        if first {
            url.push('?');
        } else {
            url.push('&');
        }
        url.push_str("cursor=");
        url.push_str(&cursor_val.to_string());
    }

    url
}

/// Map a Jetstream event to a connector trigger name.
///
/// Jetstream events have a `kind` field ("commit", "identity", "account")
/// and for commits, the operation path contains the collection.
///
/// Note: `app.bsky.feed.post` maps to `"mention"` but the caller MUST
/// also check `post_mentions_did()` to filter for actual mentions.
/// Without that filter, every post in the subscription fires the trigger.
pub fn collection_to_trigger(collection: &str) -> Option<&'static str> {
    match collection {
        "app.bsky.feed.post" => Some("mention"),
        "app.bsky.graph.follow" => Some("follow"),
        "app.bsky.feed.like" => Some("like"),
        "app.bsky.feed.repost" => Some("repost"),
        _ => None,
    }
}

/// Check if a post record mentions a specific DID.
///
/// ATProto posts store mentions in the `facets` array. Each facet has a
/// `features` array, and mention features have:
/// ```json
/// { "$type": "app.bsky.richtext.facet#mention", "did": "did:plc:..." }
/// ```
///
/// This function checks all facets for a mention feature matching `target_did`.
pub fn post_mentions_did(record: &serde_json::Value, target_did: &str) -> bool {
    let facets = match record.get("facets").and_then(|f| f.as_array()) {
        Some(f) => f,
        None => return false,
    };

    for facet in facets {
        let features = match facet.get("features").and_then(|f| f.as_array()) {
            Some(f) => f,
            None => continue,
        };

        for feature in features {
            let is_mention = feature
                .get("$type")
                .and_then(|t| t.as_str())
                .is_some_and(|t| t == "app.bsky.richtext.facet#mention");

            if is_mention {
                let did_matches = feature
                    .get("did")
                    .and_then(|d| d.as_str())
                    .is_some_and(|d| d == target_did);

                if did_matches {
                    return true;
                }
            }
        }
    }

    false
}

/// Resolve the thread root of a post record.
///
/// An AT Protocol reply record carries BOTH refs:
///
/// ```json
/// "reply": { "root": { "uri": ..., "cid": ... },
///            "parent": { "uri": ..., "cid": ... } }
/// ```
///
/// Clients group a thread by its ROOT, so a reply that names its parent
/// as the root splits off into a thread of its own. When the post is
/// itself a reply, the conversation's real root is `record.reply.root`.
/// A top-level post has no `reply` field and is the root of its own
/// thread, so it falls back to `(uri, cid)` — the post itself.
///
/// A partial/malformed `reply.root` (only one of uri/cid) also falls
/// back: a half-formed root ref is worse than rooting at the post.
pub fn thread_root(record: &serde_json::Value, uri: &str, cid: &str) -> (String, String) {
    let root = record.get("reply").and_then(|r| r.get("root"));
    let root_uri = root
        .and_then(|r| r.get("uri"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty());
    let root_cid = root
        .and_then(|r| r.get("cid"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty());

    match (root_uri, root_cid) {
        (Some(u), Some(c)) => (u.to_owned(), c.to_owned()),
        _ => (uri.to_owned(), cid.to_owned()),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_thread_root_top_level_post_is_its_own_root() {
        let record = serde_json::json!({ "text": "top level" });
        let (uri, cid) = thread_root(&record, "at://did:plc:a/app.bsky.feed.post/1", "bafyone");
        assert_eq!(uri, "at://did:plc:a/app.bsky.feed.post/1");
        assert_eq!(cid, "bafyone");
    }

    #[test]
    fn test_thread_root_reply_uses_record_root_not_parent() {
        let record = serde_json::json!({
            "text": "nested reply",
            "reply": {
                "root": { "uri": "at://did:plc:r/app.bsky.feed.post/root", "cid": "bafyroot" },
                "parent": { "uri": "at://did:plc:p/app.bsky.feed.post/mid", "cid": "bafymid" }
            }
        });
        let (uri, cid) = thread_root(&record, "at://did:plc:a/app.bsky.feed.post/1", "bafyone");
        assert_eq!(uri, "at://did:plc:r/app.bsky.feed.post/root");
        assert_eq!(cid, "bafyroot");
    }

    #[test]
    fn test_thread_root_partial_ref_falls_back_to_post() {
        let record = serde_json::json!({
            "reply": { "root": { "uri": "at://did:plc:r/app.bsky.feed.post/root" } }
        });
        let (uri, cid) = thread_root(&record, "at://did:plc:a/app.bsky.feed.post/1", "bafyone");
        assert_eq!(uri, "at://did:plc:a/app.bsky.feed.post/1");
        assert_eq!(cid, "bafyone");
    }

    #[test]
    fn test_build_jetstream_url_basic() {
        let url = build_jetstream_url(
            "wss://jetstream2.us-west.bsky.network/subscribe",
            &[COLLECTION_POST, COLLECTION_LIKE],
            None,
        );
        assert!(url.contains("wantedCollections=app.bsky.feed.post"));
        assert!(url.contains("wantedCollections=app.bsky.feed.like"));
        assert!(!url.contains("cursor="));
    }

    #[test]
    fn test_build_jetstream_url_with_cursor() {
        let url = build_jetstream_url(
            "wss://jetstream2.us-west.bsky.network/subscribe",
            &[COLLECTION_POST],
            Some(1234567890),
        );
        assert!(url.contains("cursor=1234567890"));
    }

    #[test]
    fn test_collection_to_trigger() {
        assert_eq!(collection_to_trigger("app.bsky.feed.post"), Some("mention"));
        assert_eq!(
            collection_to_trigger("app.bsky.graph.follow"),
            Some("follow")
        );
        assert_eq!(collection_to_trigger("app.bsky.feed.like"), Some("like"));
        assert_eq!(
            collection_to_trigger("app.bsky.feed.repost"),
            Some("repost")
        );
        assert_eq!(collection_to_trigger("unknown"), None);
    }

    #[test]
    fn test_post_mentions_did_found() {
        let record = serde_json::json!({
            "text": "Hello @alice",
            "facets": [{
                "features": [{
                    "$type": "app.bsky.richtext.facet#mention",
                    "did": "did:plc:alice123"
                }],
                "index": { "byteStart": 6, "byteEnd": 12 }
            }]
        });
        assert!(post_mentions_did(&record, "did:plc:alice123"));
    }

    #[test]
    fn test_post_mentions_did_not_found() {
        let record = serde_json::json!({
            "text": "Hello @alice",
            "facets": [{
                "features": [{
                    "$type": "app.bsky.richtext.facet#mention",
                    "did": "did:plc:alice123"
                }],
                "index": { "byteStart": 6, "byteEnd": 12 }
            }]
        });
        assert!(!post_mentions_did(&record, "did:plc:bob456"));
    }

    #[test]
    fn test_post_mentions_did_no_facets() {
        let record = serde_json::json!({
            "text": "Hello world, no mentions here"
        });
        assert!(!post_mentions_did(&record, "did:plc:anyone"));
    }

    #[test]
    fn test_post_mentions_did_link_facet_ignored() {
        let record = serde_json::json!({
            "text": "Check https://example.com",
            "facets": [{
                "features": [{
                    "$type": "app.bsky.richtext.facet#link",
                    "uri": "https://example.com"
                }],
                "index": { "byteStart": 6, "byteEnd": 25 }
            }]
        });
        assert!(!post_mentions_did(&record, "did:plc:anyone"));
    }

    #[test]
    fn test_post_mentions_did_multiple_facets() {
        let record = serde_json::json!({
            "text": "Hello @alice and @bob",
            "facets": [
                {
                    "features": [{
                        "$type": "app.bsky.richtext.facet#mention",
                        "did": "did:plc:alice123"
                    }],
                    "index": { "byteStart": 6, "byteEnd": 12 }
                },
                {
                    "features": [{
                        "$type": "app.bsky.richtext.facet#mention",
                        "did": "did:plc:bob456"
                    }],
                    "index": { "byteStart": 17, "byteEnd": 21 }
                }
            ]
        });
        assert!(post_mentions_did(&record, "did:plc:bob456"));
        assert!(!post_mentions_did(&record, "did:plc:charlie789"));
    }
}
