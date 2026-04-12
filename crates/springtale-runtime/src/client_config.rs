//! Thin client configuration for CLI + desktop frontends.
//!
//! Frontends (CLI, Tauri desktop, web dashboard) that talk to the daemon's
//! management API all need the same three things:
//!   1. Base URL (from `api.bind` in the config file).
//!   2. Bearer token (derived from vault passphrase, same HMAC the daemon
//!      computes at boot).
//!   3. Auth header wiring.
//!
//! This module keeps all of that in one place so no frontend reinvents it
//! (historical bug: trace.rs derived the HMAC with key/msg swapped vs the
//! server, which made it impossible to authenticate).

use std::path::Path;

use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;

use springtale_crypto::token::derive_api_token_hash;

/// Connection information for talking to the management API.
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Base URL, e.g. `http://127.0.0.1:8080`.
    pub base_url: String,
    /// Hex-encoded 32-byte API token (the computed hash — same value the
    /// daemon stores in `AppState::api_token_hash`).
    pub token_hex: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ClientConfigError {
    #[error("config file read failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("config file is not valid TOML: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("no API token — set SPRINGTALE_API_TOKEN (hex) or SPRINGTALE_PASSPHRASE")]
    NoToken,
}

/// Minimal `[api]` section we pull from the config file.
///
/// Deliberately `Deserialize`-only (never `Serialize`) per security.md.
/// Unknown fields are ignored so CLI tools don't break when the daemon
/// adds new config keys.
#[derive(Debug, Default, Deserialize)]
struct PartialConfig {
    #[serde(default)]
    api: ApiSection,
}

#[derive(Debug, Deserialize)]
struct ApiSection {
    #[serde(default = "default_bind")]
    bind: String,
}

impl Default for ApiSection {
    fn default() -> Self {
        Self {
            bind: default_bind(),
        }
    }
}

fn default_bind() -> String {
    "127.0.0.1:8080".to_owned()
}

/// Read the `api.bind` address from a config file. Returns the default
/// (`http://127.0.0.1:8080`) if the file is missing.
pub fn load_base_url(config_path: &Path) -> Result<String, ClientConfigError> {
    if !config_path.exists() {
        return Ok(format!("http://{}", default_bind()));
    }
    let text = std::fs::read_to_string(config_path)?;
    let cfg: PartialConfig = toml::from_str(&text)?;
    Ok(format!("http://{}", cfg.api.bind))
}

/// Derive the hex-encoded API token from a passphrase.
///
/// This MUST match the server-side derivation in
/// `springtale_crypto::token::derive_api_token_hash`, which the daemon
/// calls at boot to populate `AppState::api_token_hash`. Use this helper
/// exclusively — do not reimplement the HMAC.
pub fn token_from_passphrase(passphrase: &SecretString) -> String {
    let hash = derive_api_token_hash(passphrase.expose_secret().as_bytes());
    // SECURITY: expose needed to feed passphrase bytes into the HMAC.
    // The resulting hash is NOT secret in the threat model (it's what
    // the daemon stores in memory and compares against).
    hex::encode(hash)
}

/// Resolve the API token from the environment.
///
/// Resolution order:
///   1. `SPRINGTALE_API_TOKEN` — already hex, used verbatim.
///   2. `SPRINGTALE_PASSPHRASE` — passed through the HMAC derivation.
///   3. `None` — caller is expected to prompt interactively and call
///      [`token_from_passphrase`] directly.
pub fn token_from_env() -> Option<String> {
    if let Ok(token) = std::env::var("SPRINGTALE_API_TOKEN")
        && !token.is_empty()
    {
        return Some(token);
    }
    if let Ok(pass) = std::env::var("SPRINGTALE_PASSPHRASE")
        && !pass.is_empty()
    {
        let secret = SecretString::new(pass.into());
        return Some(token_from_passphrase(&secret));
    }
    None
}

/// Determine whether a user-supplied string is already a valid hex token
/// (64 hex chars = 32 bytes). Used by CLI prompts that accept either a
/// raw token or a passphrase.
pub fn looks_like_hex_token(input: &str) -> bool {
    input.len() == 64 && input.chars().all(|c| c.is_ascii_hexdigit())
}
