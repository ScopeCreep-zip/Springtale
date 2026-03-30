use base64::Engine;
use rsa::pkcs1v15::VerifyingKey;
use rsa::signature::Verifier;
use rsa::{pkcs8::DecodePublicKey, RsaPublicKey};
use sha2::Sha256;

use crate::error::KickError;

/// Verify a Kick webhook payload signature.
///
/// Kick signs webhooks with RSA-PKCS1v15 over SHA256. The process:
/// 1. Concatenate `{message_id}.{timestamp}.{body}` with dots
/// 2. Hash with SHA256
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
    // Parse the RSA public key from PEM
    let public_key = RsaPublicKey::from_public_key_pem(public_key_pem)
        .map_err(|e| KickError::RequestFailed(format!("invalid public key: {e}")))?;

    // Construct the signed message: "{message_id}.{timestamp}.{body}"
    let body_str = std::str::from_utf8(body)
        .map_err(|e| KickError::RequestFailed(format!("webhook body is not UTF-8: {e}")))?;
    let message = format!("{message_id}.{timestamp}.{body_str}");

    // Decode the base64 signature
    let signature_bytes = base64::engine::general_purpose::STANDARD
        .decode(signature_b64)
        .map_err(|e| KickError::RequestFailed(format!("invalid signature encoding: {e}")))?;

    // Verify RSA-PKCS1v15 with SHA256
    let verifying_key = VerifyingKey::<Sha256>::new(public_key);
    let signature = rsa::pkcs1v15::Signature::try_from(signature_bytes.as_slice())
        .map_err(|e| KickError::RequestFailed(format!("invalid signature format: {e}")))?;

    verifying_key
        .verify(message.as_bytes(), &signature)
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
        assert_eq!(event_type_to_trigger("chat.message.sent"), Some("chat_message"));
        assert_eq!(event_type_to_trigger("channel.followed"), Some("channel_followed"));
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
