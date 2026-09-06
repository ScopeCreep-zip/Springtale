//! Thin client configuration for CLI + desktop frontends.
//!
//! Frontends (CLI, Tauri desktop, web dashboard) that talk to the daemon's
//! management API all need the same three things:
//!   1. Base URL (from `api.bind` in the config file).
//!   2. Bearer token — one the daemon ISSUED (`springtale login`, plan
//!      6.6). Nothing derives a bearer from the vault passphrase any
//!      more; the passphrase-derived hash is the login verifier only.
//!   3. Auth header wiring.
//!
//! This module keeps all of that in one place so no frontend reinvents it
//! (historical bug: trace.rs derived the HMAC with key/msg swapped vs the
//! server, which made it impossible to authenticate).

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use specta::Type;

/// Connection information for talking to the management API.
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Base URL, e.g. `http://127.0.0.1:8080`.
    pub base_url: String,
    /// Hex-encoded 32-byte API token the daemon issued.
    pub token_hex: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ClientConfigError {
    #[error("config file read failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("config file is not valid TOML: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("not logged in — run `springtale login` (or set SPRINGTALE_API_TOKEN)")]
    NoToken,
    #[error("no config directory — set XDG_CONFIG_HOME or HOME")]
    NoConfigDir,
    #[error("saved token file is not valid: {0}")]
    BadTokenFile(String),
}

/// Minimal `[api]` section we pull from the config file.
///
/// Deliberately `Deserialize`-only (never `Serialize`) per security.md.
/// Unknown fields are ignored so CLI tools don't break when the daemon
/// adds new config keys.
#[derive(Debug, Default, Deserialize, Type)]
struct PartialConfig {
    #[serde(default)]
    api: ApiSection,
}

#[derive(Debug, Deserialize, Type)]
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

/// Where `springtale login` saves the token it was issued.
///
/// `$XDG_CONFIG_HOME/springtale/token`, falling back to
/// `$HOME/.config/springtale/token` — the XDG basedir rule.
pub fn token_file_path() -> Result<PathBuf, ClientConfigError> {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME")
        && !xdg.is_empty()
    {
        return Ok(PathBuf::from(xdg).join("springtale").join("token"));
    }
    let home = std::env::var("HOME").map_err(|_| ClientConfigError::NoConfigDir)?;
    if home.is_empty() {
        return Err(ClientConfigError::NoConfigDir);
    }
    Ok(PathBuf::from(home)
        .join(".config")
        .join("springtale")
        .join("token"))
}

/// What `springtale login` stores: the issued token plus the id that
/// revokes it. The id is what `springtale logout` hands to
/// `DELETE /auth/tokens/{id}`.
#[derive(Debug, Clone, Deserialize)]
pub struct SavedToken {
    /// `api_tokens.id` — the revocation handle.
    pub id: String,
    /// Hex-encoded issued token.
    pub token: String,
}

/// Read the saved token file. `Ok(None)` when there is no file.
pub fn read_token_file() -> Result<Option<SavedToken>, ClientConfigError> {
    let path = token_file_path()?;
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path)?;
    let saved: SavedToken = serde_json::from_str(text.trim())
        .map_err(|e| ClientConfigError::BadTokenFile(e.to_string()))?;
    if !looks_like_hex_token(&saved.token) {
        return Err(ClientConfigError::BadTokenFile(
            "token is not 64 hex characters".to_owned(),
        ));
    }
    Ok(Some(saved))
}

/// Write the token file with mode `0600` — it is a live credential.
///
/// The file is created with the restrictive mode from the start (not
/// chmod'ed afterwards), so it is never briefly world-readable.
pub fn write_token_file(id: &str, token: &str) -> Result<PathBuf, ClientConfigError> {
    let path = token_file_path()?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
        }
    }
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let body = serde_json::json!({ "id": id, "token": token }).to_string();
    let mut file = opts.open(&path)?;
    file.write_all(body.as_bytes())?;
    file.flush()?;
    Ok(path)
}

/// Delete the saved token file. Returns whether one was there.
pub fn delete_token_file() -> Result<bool, ClientConfigError> {
    let path = token_file_path()?;
    if !path.exists() {
        return Ok(false);
    }
    std::fs::remove_file(&path)?;
    Ok(true)
}

/// Resolve the API token from the environment.
///
/// `SPRINGTALE_API_TOKEN` only, and it must be a token the daemon issued.
/// The old `SPRINGTALE_PASSPHRASE` branch — which HMAC'd the passphrase
/// into a bearer — is gone (plan 6.6): a passphrase logs in, it is not
/// itself a credential the API accepts.
pub fn token_from_env() -> Option<String> {
    match std::env::var("SPRINGTALE_API_TOKEN") {
        Ok(token) if !token.is_empty() => Some(token),
        _ => None,
    }
}

/// Determine whether a user-supplied string is already a valid hex token
/// (64 hex chars = 32 bytes). Used by CLI prompts that accept either a
/// raw token or a passphrase.
pub fn looks_like_hex_token(input: &str) -> bool {
    input.len() == 64 && input.chars().all(|c| c.is_ascii_hexdigit())
}
