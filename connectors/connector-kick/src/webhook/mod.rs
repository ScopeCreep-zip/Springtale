use base64::Engine;
// SECURITY (E3 audit-fix): switched from `rsa` crate to `ring` for the
// signature verification path. The `rsa` crate's PKCS1v15 implementation
// has RUSTSEC-2023-0071 (Marvin Attack timing sidechannel) with no
// upstream fix. The Marvin Attack only affects PKCS1v15 *decryption*,
// which this module never invokes — but the cargo-deny ban
// (`deny.toml`) blocks the whole crate so we replaced the dep entirely.
// `ring` provides constant-time RSA-PKCS1v15-SHA256 verification via
// `signature::RSA_PKCS1_2048_8192_SHA256`, used by rustls itself.
use ring::signature;
use rustls_pemfile::Item;

use crate::error::KickError;

pub mod ingest;
pub mod replay;

pub use ingest::ingest_event;
pub use replay::{ReplayCache, check_timestamp};

/// Header carrying the idempotent message id (`Kick-Event-Message-Id`).
pub const HEADER_MESSAGE_ID: &str = "kick-event-message-id";
/// Header carrying the RFC 3339 send time (`Kick-Event-Message-Timestamp`).
pub const HEADER_TIMESTAMP: &str = "kick-event-message-timestamp";
/// Header carrying the base64 RSA signature (`Kick-Event-Signature`).
pub const HEADER_SIGNATURE: &str = "kick-event-signature";

/// Look up a required webhook header, case-insensitively (the daemon
/// hands connectors lower-cased names; other callers may not).
pub fn required_header<'a>(
    headers: &'a std::collections::HashMap<String, String>,
    name: &str,
) -> Result<&'a str, KickError> {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
        .ok_or_else(|| KickError::RequestFailed(format!("missing {name} header")))
}

/// Verify a Kick webhook payload signature.
///
/// Kick signs webhooks with RSA-PKCS1v15 over SHA256. The process:
/// 1. Concatenate `{message_id}.{timestamp}.{body}` with dots
/// 2. Hash with SHA256 (ring computes this internally as part of verify)
/// 3. Verify RSA-PKCS1v15 signature using Kick's public key
///
/// # Arguments
/// * `public_key_pem` — PEM-encoded RSA public key from `GET /public/v1/public-key`
/// * `message_id` — `Kick-Event-Message-Id` header
/// * `timestamp` — `Kick-Event-Message-Timestamp` header
/// * `body` — raw request body bytes
/// * `signature_b64` — `Kick-Event-Signature` header (base64-encoded)
pub fn verify_webhook(
    public_key_pem: &str,
    message_id: &str,
    timestamp: &str,
    body: &[u8],
    signature_b64: &str,
) -> Result<(), KickError> {
    // Extract DER-encoded SubjectPublicKeyInfo from the PEM blob.
    let mut reader = std::io::BufReader::new(public_key_pem.as_bytes());
    let item = rustls_pemfile::read_one(&mut reader)
        .map_err(|e| KickError::RequestFailed(format!("invalid public key PEM: {e}")))?
        .ok_or_else(|| KickError::RequestFailed("public key PEM is empty".to_owned()))?;
    let spki_der = match item {
        Item::SubjectPublicKeyInfo(spki) => spki,
        _ => {
            return Err(KickError::RequestFailed(
                "public key PEM must contain a SubjectPublicKeyInfo block".to_owned(),
            ));
        }
    };

    // Construct the signed message: "{message_id}.{timestamp}.{body}"
    let body_str = std::str::from_utf8(body)
        .map_err(|e| KickError::RequestFailed(format!("webhook body is not UTF-8: {e}")))?;
    let message = format!("{message_id}.{timestamp}.{body_str}");

    // Decode the base64 signature
    let signature_bytes = base64::engine::general_purpose::STANDARD
        .decode(signature_b64)
        .map_err(|e| KickError::RequestFailed(format!("invalid signature encoding: {e}")))?;

    // Verify RSA-PKCS1v15 with SHA256. ring's verifier is constant-time
    // and does not exercise the Marvin Attack code path.
    let public_key = signature::UnparsedPublicKey::new(
        &signature::RSA_PKCS1_2048_8192_SHA256,
        spki_der.as_ref(),
    );
    public_key
        .verify(message.as_bytes(), &signature_bytes)
        .map_err(|_| KickError::RequestFailed("webhook signature verification failed".to_owned()))
}

/// Map a Kick webhook event type to a connector trigger name.
///
/// Most events map 1:1. The exception is `livestream.status.updated` which
/// maps to either `stream_live` or `stream_offline` based on the `is_live`
/// field in the payload — use `resolve_livestream_trigger()` for that.
pub fn event_type_to_trigger(event_type: &str) -> Option<&'static str> {
    match event_type {
        "chat.message.sent" => Some("chat_message"),
        "channel.followed" => Some("channel_followed"),
        // livestream.status.updated requires payload inspection — use
        // resolve_livestream_trigger() instead
        "livestream.status.updated" => None,
        _ => None,
    }
}

/// Resolve the trigger name for a `livestream.status.updated` event.
///
/// Kick sends a single event type for both going live and going offline.
/// The `is_live` boolean in the payload determines which trigger fires.
pub fn resolve_livestream_trigger(payload: &serde_json::Value) -> Option<&'static str> {
    match payload.get("is_live").and_then(|v| v.as_bool()) {
        Some(true) => Some("stream_live"),
        Some(false) => Some("stream_offline"),
        None => None,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_event_type_to_trigger() {
        assert_eq!(
            event_type_to_trigger("chat.message.sent"),
            Some("chat_message")
        );
        assert_eq!(
            event_type_to_trigger("channel.followed"),
            Some("channel_followed")
        );
        // livestream requires payload inspection, returns None here
        assert_eq!(event_type_to_trigger("livestream.status.updated"), None);
        assert_eq!(event_type_to_trigger("unknown.event"), None);
    }

    #[test]
    fn test_resolve_livestream_trigger_live() {
        let payload = serde_json::json!({ "is_live": true, "title": "test stream" });
        assert_eq!(resolve_livestream_trigger(&payload), Some("stream_live"));
    }

    #[test]
    fn test_resolve_livestream_trigger_offline() {
        let payload = serde_json::json!({ "is_live": false });
        assert_eq!(resolve_livestream_trigger(&payload), Some("stream_offline"));
    }

    #[test]
    fn test_resolve_livestream_trigger_missing_field() {
        let payload = serde_json::json!({ "title": "no is_live field" });
        assert_eq!(resolve_livestream_trigger(&payload), None);
    }

    #[test]
    fn test_verify_webhook_rejects_invalid_key() {
        let result = verify_webhook(
            "not a valid PEM key",
            "msg-123",
            "1234567890",
            b"{}",
            "dGVzdA==", // base64("test")
        );
        assert!(result.is_err());
    }
}
