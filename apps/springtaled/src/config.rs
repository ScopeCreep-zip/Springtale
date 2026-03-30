use std::path::PathBuf;

use figment::providers::{Env, Format, Toml};
use figment::Figment;
use garde::Validate;
use serde::Deserialize;
use springtale_store::paths;

/// Top-level daemon configuration.
///
/// Loaded from `springtale.toml` with env var overrides (SPRINGTALE_ prefix).
/// Validated with garde after deserialization (architecture doc §8.1).
#[derive(Debug, Deserialize, Validate)]
pub struct SpringtaleConfig {
    #[serde(default)]
    #[garde(dive)]
    pub store: StoreConfig,
    #[serde(default)]
    #[garde(dive)]
    pub crypto: CryptoConfig,
    #[serde(default)]
    #[garde(dive)]
    pub transport: TransportConfig,
    #[serde(default)]
    #[garde(dive)]
    pub api: ApiConfig,
}

#[derive(Debug, Deserialize, Validate)]
pub struct StoreConfig {
    /// Path to the SQLite database file.
    #[serde(default = "paths::default_db_path")]
    #[garde(custom(validate_path))]
    pub path: PathBuf,
}

impl Default for StoreConfig {
    fn default() -> Self {
        Self {
            path: paths::default_db_path(),
        }
    }
}

#[derive(Debug, Deserialize, Validate)]
pub struct CryptoConfig {
    /// Path to the encrypted vault file.
    #[serde(default = "paths::default_vault_path")]
    #[garde(custom(validate_path))]
    pub vault_path: PathBuf,
}

impl Default for CryptoConfig {
    fn default() -> Self {
        Self {
            vault_path: paths::default_vault_path(),
        }
    }
}

#[derive(Debug, Deserialize, Validate)]
pub struct TransportConfig {
    /// Path to the Unix domain socket.
    #[serde(default = "paths::default_socket_path")]
    #[garde(custom(validate_path))]
    pub socket_path: PathBuf,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            socket_path: paths::default_socket_path(),
        }
    }
}

#[derive(Debug, Deserialize, Validate)]
pub struct ApiConfig {
    /// Bind address for the management API. Default: 127.0.0.1:8080.
    /// WARNING: binding to 0.0.0.0 exposes the API to the network.
    #[serde(default = "default_bind")]
    #[garde(length(min = 1))]
    pub bind: String,

    /// Maximum requests per second (rate limiting). Default: 100.
    #[serde(default = "default_rate_limit")]
    #[garde(range(min = 1, max = 10000))]
    pub rate_limit_per_sec: u32,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            bind: default_bind(),
            rate_limit_per_sec: default_rate_limit(),
        }
    }
}

fn default_bind() -> String {
    "127.0.0.1:8080".to_owned()
}

fn default_rate_limit() -> u32 {
    100
}

/// Validate that a path is absolute and does not contain parent directory references.
/// Prevents path traversal attacks via config injection.
#[allow(clippy::ptr_arg)] // garde derive macro passes &PathBuf, not &Path
fn validate_path(value: &PathBuf, _ctx: &()) -> garde::Result {
    if !value.is_absolute() {
        return Err(garde::Error::new("path must be absolute"));
    }
    if value
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(garde::Error::new("path must not contain '..'"));
    }
    Ok(())
}

/// Load configuration from TOML file + environment variable overrides.
///
/// Priority (highest wins):
/// 1. Environment variables (SPRINGTALE_ prefix, underscore-separated nesting)
/// 2. springtale.toml in current directory
/// 3. Built-in defaults
///
/// After deserialization, validates with garde per architecture doc §8.1.
pub fn load_config() -> Result<SpringtaleConfig, anyhow::Error> {
    let config: SpringtaleConfig = Figment::new()
        .merge(Toml::file("springtale.toml"))
        // Use __ (double underscore) as nesting separator to preserve
        // single underscores in field names like vault_path, socket_path.
        // Example: SPRINGTALE_CRYPTO__VAULT_PATH=/path sets crypto.vault_path
        .merge(Env::prefixed("SPRINGTALE_").map(|key| {
            key.as_str().replace("__", ".").into()
        }))
        .extract()?;

    // Validate configuration (garde)
    config.validate()?;

    Ok(config)
}
