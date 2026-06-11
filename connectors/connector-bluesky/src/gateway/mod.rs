//! Bluesky Jetstream firehose gateway.
//!
//! Jetstream is Bluesky's official simplified firehose: a WebSocket that
//! streams ATProto repo commits as JSON (no CBOR/MST decoding needed).
//! We subscribe to `app.bsky.feed.post` and classify each `create` commit:
//!
//! - the post is authored by OUR account (`did == own_did`) → `own_post`
//! - the post @mentions us (a `facet#mention` with our did) → `mention`
//! - otherwise it's some other user's post → ignored.
//!
//! Mentions necessarily require the full post stream (they come from
//! arbitrary accounts), so the firehose is filtered client-side here.
//! See <https://docs.bsky.app/blog/jetstream> and
//! <https://github.com/bluesky-social/jetstream>.

use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use crate::firehose;

/// Reconnect delay after a dropped/failed Jetstream connection.
const RECONNECT_DELAY: Duration = Duration::from_secs(5);

/// Connect to Jetstream and stream post commits until shutdown,
/// reconnecting on drops. Each relevant event is routed to a flat
/// trigger payload and handed to `dispatcher`.
pub async fn gateway_loop(
    jetstream_url: String,
    own_did: String,
    dispatcher: Arc<dyn Fn(serde_json::Value) + Send + Sync>,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) {
    let url = firehose::build_jetstream_url(&jetstream_url, &[firehose::COLLECTION_POST], None);
    tracing::info!(did = %own_did, "Bluesky Jetstream gateway loop started");

    loop {
        let ws_stream: WebSocketStream<MaybeTlsStream<TcpStream>> =
            match tokio_tungstenite::connect_async(&url).await {
                Ok((stream, _)) => stream,
                Err(e) => {
                    tracing::error!(error = %e, "Jetstream WebSocket connection failed");
                    if wait_or_shutdown(&mut shutdown_rx).await {
                        return;
                    }
                    continue;
                }
            };

        let (mut ws_tx, mut ws_rx) = ws_stream.split();
        tracing::info!("Bluesky Jetstream connected");

        loop {
            tokio::select! {
                msg = ws_rx.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            if let Ok(event) = serde_json::from_str::<serde_json::Value>(text.as_str())
                                && let Some(payload) = route_jetstream_event(&event, &own_did)
                            {
                                dispatcher(payload);
                            }
                        }
                        Some(Ok(Message::Ping(data))) => {
                            if let Err(e) = ws_tx.send(Message::Pong(data)).await {
                                tracing::warn!(error = %e, "Jetstream pong failed");
                                break;
                            }
                        }
                        Some(Ok(Message::Close(_))) | None => {
                            tracing::info!("Jetstream connection closed");
                            break;
                        }
                        Some(Err(e)) => {
                            tracing::warn!(error = %e, "Jetstream WebSocket error");
                            break;
                        }
                        _ => {} // Binary / Pong / Frame — ignore
                    }
                }
                _ = shutdown_rx.changed() => {
                    tracing::info!("Bluesky Jetstream gateway shutting down");
                    let _ = ws_tx.send(Message::Close(None)).await;
                    return;
                }
            }
        }

        if wait_or_shutdown(&mut shutdown_rx).await {
            return;
        }
    }
}

/// Sleep the reconnect delay, returning `true` if shutdown fired first.
async fn wait_or_shutdown(shutdown_rx: &mut tokio::sync::watch::Receiver<bool>) -> bool {
    tokio::select! {
        _ = tokio::time::sleep(RECONNECT_DELAY) => false,
        _ = shutdown_rx.changed() => true,
    }
}

/// Classify a Jetstream event into a flat trigger payload, or `None` if
/// it isn't a post `create` commit relevant to us (our own post, or a
/// post mentioning us). Pure — unit-tested without a live connection.
pub fn route_jetstream_event(
    event: &serde_json::Value,
    own_did: &str,
) -> Option<serde_json::Value> {
    if event.get("kind").and_then(|k| k.as_str()) != Some("commit") {
        return None;
    }
    let did = event.get("did").and_then(|d| d.as_str())?;
    let commit = event.get("commit")?;
    if commit.get("operation").and_then(|o| o.as_str()) != Some("create") {
        return None;
    }
    if commit.get("collection").and_then(|c| c.as_str()) != Some(firehose::COLLECTION_POST) {
        return None;
    }
    let record = commit.get("record")?;

    let trigger = if did == own_did {
        "own_post"
    } else if firehose::post_mentions_did(record, own_did) {
        "mention"
    } else {
        return None;
    };

    let rkey = commit
        .get("rkey")
        .and_then(|r| r.as_str())
        .unwrap_or_default();

    Some(serde_json::json!({
        "trigger": trigger,
        "did": did,
        "text": record.get("text").and_then(|t| t.as_str()).unwrap_or_default(),
        "uri": format!("at://{did}/app.bsky.feed.post/{rkey}"),
        "cid": commit.get("cid").and_then(|c| c.as_str()).unwrap_or_default(),
        "created_at": event.get("time_us").cloned().unwrap_or(serde_json::Value::Null),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const OWN: &str = "did:plc:me";

    // Real Jetstream commit event shape for an `app.bsky.feed.post` create.
    fn post_commit(author_did: &str, record: serde_json::Value) -> serde_json::Value {
        json!({
            "did": author_did,
            "time_us": 1_700_000_000_000_000u64,
            "kind": "commit",
            "commit": {
                "rev": "abc",
                "operation": "create",
                "collection": "app.bsky.feed.post",
                "rkey": "3kxyz",
                "cid": "bafyreigh2akiscaildc",
                "record": record
            }
        })
    }

    #[test]
    fn classifies_own_post() {
        let event = post_commit(OWN, json!({ "$type": "app.bsky.feed.post", "text": "gm" }));
        let p = route_jetstream_event(&event, OWN).expect("own post routes");
        assert_eq!(p["trigger"], "own_post");
        assert_eq!(p["text"], "gm");
        assert_eq!(p["uri"], "at://did:plc:me/app.bsky.feed.post/3kxyz");
        assert_eq!(p["cid"], "bafyreigh2akiscaildc");
    }

    #[test]
    fn classifies_mention() {
        let record = json!({
            "$type": "app.bsky.feed.post",
            "text": "hey @me",
            "facets": [{
                "features": [{ "$type": "app.bsky.richtext.facet#mention", "did": OWN }],
                "index": { "byteStart": 4, "byteEnd": 7 }
            }]
        });
        let event = post_commit("did:plc:someone", record);
        let p = route_jetstream_event(&event, OWN).expect("mention routes");
        assert_eq!(p["trigger"], "mention");
        assert_eq!(p["text"], "hey @me");
        assert_eq!(p["uri"], "at://did:plc:someone/app.bsky.feed.post/3kxyz");
    }

    #[test]
    fn ignores_unrelated_post() {
        let event = post_commit(
            "did:plc:stranger",
            json!({ "text": "a post about nothing" }),
        );
        assert!(route_jetstream_event(&event, OWN).is_none());
    }

    #[test]
    fn ignores_non_commit_and_deletes() {
        assert!(route_jetstream_event(&json!({ "kind": "identity", "did": OWN }), OWN).is_none());
        let mut del = post_commit(OWN, json!({ "text": "x" }));
        del["commit"]["operation"] = json!("delete");
        assert!(route_jetstream_event(&del, OWN).is_none());
    }

    #[test]
    fn ignores_non_post_collections() {
        let mut like = post_commit(OWN, json!({ "text": "x" }));
        like["commit"]["collection"] = json!("app.bsky.feed.like");
        assert!(route_jetstream_event(&like, OWN).is_none());
    }
}
