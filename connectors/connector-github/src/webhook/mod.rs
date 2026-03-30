use hmac::{Hmac, Mac};
use secrecy::ExposeSecret;
use sha2::Sha256;

use crate::error::GithubError;

type HmacSha256 = Hmac<Sha256>;

/// Verify a GitHub webhook payload signature.
///
/// GitHub sends `X-Hub-Signature-256: sha256=<hex-encoded HMAC>` with
/// each webhook delivery. This function recomputes the HMAC-SHA256 and
/// uses constant-time comparison to prevent timing attacks.
///
/// # Arguments
/// * `secret` — the webhook secret configured in GitHub
/// * `payload` — the raw request body bytes
/// * `signature_header` — the `X-Hub-Signature-256` header value (e.g., `sha256=abc123...`)
pub fn verify_signature(
    secret: &secrecy::SecretBox<String>,
    payload: &[u8],
    signature_header: &str,
) -> Result<(), GithubError> {
    let hex_sig = signature_header
        .strip_prefix("sha256=")
        .ok_or(GithubError::WebhookSignatureInvalid)?;

    let expected_sig = hex::decode(hex_sig)
        .map_err(|_| GithubError::WebhookSignatureInvalid)?;

    // SECURITY: expose needed for HMAC key computation
    let mut mac = HmacSha256::new_from_slice(secret.expose_secret().as_bytes())
        .map_err(|_| GithubError::WebhookSignatureInvalid)?;
    mac.update(payload);

    // Constant-time comparison via the `verify_slice` method
    mac.verify_slice(&expected_sig)
        .map_err(|_| GithubError::WebhookSignatureInvalid)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use secrecy::SecretBox;

    #[test]
    fn test_verify_valid_signature() {
        let secret = SecretBox::new(Box::new("test-secret".to_owned()));
        let payload = b"hello world";

        // Compute expected signature
        let mut mac = HmacSha256::new_from_slice(b"test-secret").unwrap();
        mac.update(payload);
        let result = mac.finalize();
        let hex_sig = hex::encode(result.into_bytes());
        let header = format!("sha256={hex_sig}");

        assert!(verify_signature(&secret, payload, &header).is_ok());
    }

    #[test]
    fn test_verify_invalid_signature() {
        let secret = SecretBox::new(Box::new("test-secret".to_owned()));
        let payload = b"hello world";

        let header = "sha256=0000000000000000000000000000000000000000000000000000000000000000";

        let result = verify_signature(&secret, payload, header);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), GithubError::WebhookSignatureInvalid));
    }

    #[test]
    fn test_verify_missing_prefix() {
        let secret = SecretBox::new(Box::new("test-secret".to_owned()));
        let payload = b"hello world";

        let result = verify_signature(&secret, payload, "bad_prefix=abc123");
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_invalid_hex() {
        let secret = SecretBox::new(Box::new("test-secret".to_owned()));
        let payload = b"hello world";

        let result = verify_signature(&secret, payload, "sha256=not_hex!");
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_different_payload_fails() {
        let secret = SecretBox::new(Box::new("test-secret".to_owned()));

        let mut mac = HmacSha256::new_from_slice(b"test-secret").unwrap();
        mac.update(b"original payload");
        let result = mac.finalize();
        let hex_sig = hex::encode(result.into_bytes());
        let header = format!("sha256={hex_sig}");

        // Different payload should fail
        assert!(verify_signature(&secret, b"tampered payload", &header).is_err());
    }
}
