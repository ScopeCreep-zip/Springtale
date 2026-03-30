use rand::Rng;
use secrecy::ExposeSecret;
use sha2::{Digest, Sha256};

use crate::config::KickConfig;
use crate::error::KickError;

/// PKCE code verifier and challenge pair for OAuth 2.1.
pub struct PkceChallenge {
    /// The code verifier — secret, must not be logged.
    pub verifier: secrecy::SecretBox<String>,
    /// The code challenge (SHA256 of verifier, base64url-encoded). Public.
    pub challenge: String,
}

impl std::fmt::Debug for PkceChallenge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PkceChallenge")
            .field("verifier", &"[REDACTED]")
            .field("challenge", &self.challenge)
            .finish()
    }
}

/// Generate a PKCE code verifier and challenge (S256 method).
///
/// Per RFC 7636:
/// - verifier: 32 random bytes, base64url-encoded (43 characters)
/// - challenge: SHA256(verifier), base64url-encoded
pub fn generate_pkce() -> PkceChallenge {
    let mut rng = rand::thread_rng();
    let mut bytes = [0u8; 32];
    rng.fill(&mut bytes);

    let verifier = base64url_encode(&bytes);

    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let hash = hasher.finalize();
    let challenge = base64url_encode(&hash);

    PkceChallenge {
        verifier: secrecy::SecretBox::new(Box::new(verifier)),
        challenge,
    }
}

/// Build the OAuth 2.1 authorization URL for the Kick PKCE flow.
///
/// Per Kick docs: `GET https://id.kick.com/oauth/authorize`
pub fn build_authorize_url(config: &KickConfig, pkce: &PkceChallenge, state: &str) -> String {
    let scopes = config.scopes.join(" ");
    format!(
        "{}/oauth/authorize?client_id={}&redirect_uri={}&response_type=code&scope={}&code_challenge={}&code_challenge_method=S256&state={}",
        config.oauth_base,
        urlencoded(&config.client_id),
        urlencoded(&config.redirect_uri),
        urlencoded(&scopes),
        urlencoded(&pkce.challenge),
        urlencoded(state),
    )
}

/// Exchange an authorization code for an access token.
///
/// Per Kick docs: `POST https://id.kick.com/oauth/token`
/// HTTP call delegated to `KickClient::exchange_token` (all reqwest in client/).
pub async fn exchange_code(
    config: &KickConfig,
    code: &str,
    pkce_verifier: &str,
) -> Result<TokenResponse, KickError> {
    // Build URL-encoded form body
    // SECURITY: expose needed for OAuth token exchange
    let form_body = format!(
        "grant_type=authorization_code&client_id={}&client_secret={}&redirect_uri={}&code={}&code_verifier={}",
        urlencoded(&config.client_id),
        urlencoded(config.client_secret.expose_secret()),
        urlencoded(&config.redirect_uri),
        urlencoded(code),
        urlencoded(pkce_verifier),
    );

    // Delegate HTTP call to client module (no raw reqwest in auth/)
    let body = crate::client::KickClient::exchange_token(
        &config.oauth_base,
        form_body,
    )
    .await?;

    serde_json::from_str(&body)
        .map_err(|e| KickError::AuthFailed(format!("failed to parse token response: {e}")))
}

/// OAuth token response from Kick.
///
/// Deserialized from JSON. Tokens are not exposed as public fields —
/// access via `access_token()` which wraps in `SecretBox`.
#[derive(serde::Deserialize)]
pub struct TokenResponse {
    access_token: String,
    pub token_type: String,
    pub expires_in: u64,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
}

impl TokenResponse {
    /// Get the access token wrapped in SecretBox.
    pub fn access_token(&self) -> secrecy::SecretBox<String> {
        secrecy::SecretBox::new(Box::new(self.access_token.clone()))
    }

    /// Get the refresh token wrapped in SecretBox, if present.
    pub fn refresh_token(&self) -> Option<secrecy::SecretBox<String>> {
        self.refresh_token
            .as_ref()
            .map(|t| secrecy::SecretBox::new(Box::new(t.clone())))
    }
}

impl std::fmt::Debug for TokenResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenResponse")
            .field("access_token", &"[REDACTED]")
            .field("token_type", &self.token_type)
            .field("expires_in", &self.expires_in)
            .field("refresh_token", &self.refresh_token.as_ref().map(|_| "[REDACTED]"))
            .field("scope", &self.scope)
            .finish()
    }
}

/// Base64url encoding without padding (RFC 4648 §5).
fn base64url_encode(data: &[u8]) -> String {
    let mut result = String::with_capacity(data.len() * 4 / 3 + 4);
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

    let mut i = 0;
    while i + 2 < data.len() {
        let n = (u32::from(data[i]) << 16) | (u32::from(data[i + 1]) << 8) | u32::from(data[i + 2]);
        result.push(char::from(alphabet[((n >> 18) & 0x3F) as usize]));
        result.push(char::from(alphabet[((n >> 12) & 0x3F) as usize]));
        result.push(char::from(alphabet[((n >> 6) & 0x3F) as usize]));
        result.push(char::from(alphabet[(n & 0x3F) as usize]));
        i += 3;
    }

    let remaining = data.len() - i;
    if remaining == 1 {
        let n = u32::from(data[i]) << 16;
        result.push(char::from(alphabet[((n >> 18) & 0x3F) as usize]));
        result.push(char::from(alphabet[((n >> 12) & 0x3F) as usize]));
    } else if remaining == 2 {
        let n = (u32::from(data[i]) << 16) | (u32::from(data[i + 1]) << 8);
        result.push(char::from(alphabet[((n >> 18) & 0x3F) as usize]));
        result.push(char::from(alphabet[((n >> 12) & 0x3F) as usize]));
        result.push(char::from(alphabet[((n >> 6) & 0x3F) as usize]));
    }

    result
}

/// Simple percent-encoding for URL parameters.
fn urlencoded(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => result.push(c),
            ' ' => result.push_str("%20"),
            _ => {
                for byte in c.to_string().as_bytes() {
                    result.push('%');
                    result.push_str(&format!("{byte:02X}"));
                }
            }
        }
    }
    result
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use secrecy::ExposeSecret;

    #[test]
    fn test_generate_pkce() {
        let pkce = generate_pkce();
        let verifier = pkce.verifier.expose_secret();
        // Verifier should be base64url without padding
        assert!(!verifier.is_empty());
        assert!(!pkce.challenge.is_empty());
        assert!(!verifier.contains('='));
        assert!(!pkce.challenge.contains('='));
        // Verifier and challenge should be different
        assert_ne!(verifier, &pkce.challenge);
    }

    #[test]
    fn test_pkce_deterministic_challenge() {
        // Same verifier should produce same challenge
        let verifier = "test_verifier_string";
        let mut hasher = Sha256::new();
        hasher.update(verifier.as_bytes());
        let hash = hasher.finalize();
        let challenge1 = base64url_encode(&hash);

        let mut hasher2 = Sha256::new();
        hasher2.update(verifier.as_bytes());
        let hash2 = hasher2.finalize();
        let challenge2 = base64url_encode(&hash2);

        assert_eq!(challenge1, challenge2);
    }

    #[test]
    fn test_base64url_encode() {
        // Known test vector
        let encoded = base64url_encode(b"Hello");
        assert!(!encoded.contains('+'));
        assert!(!encoded.contains('/'));
        assert!(!encoded.contains('='));
    }

    #[test]
    fn test_build_authorize_url() {
        let config = KickConfig {
            client_id: "test_client_id".to_owned(),
            client_secret: secrecy::SecretBox::new(Box::new("secret".to_owned())),
            redirect_uri: "http://localhost:3000/callback".to_owned(),
            scopes: vec!["user:read".to_owned(), "chat:write".to_owned()],
            api_base: "https://api.kick.com".to_owned(),
            oauth_base: "https://id.kick.com".to_owned(),
            webhook_callback_url: None,
        };
        let pkce = generate_pkce();
        let url = build_authorize_url(&config, &pkce, "random_state");

        assert!(url.starts_with("https://id.kick.com/oauth/authorize"));
        assert!(url.contains("client_id=test_client_id"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("state=random_state"));
    }
}
