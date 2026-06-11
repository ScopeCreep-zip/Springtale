use std::path::PathBuf;

use figment::Figment;
use figment::providers::{Env, Format, Toml};
use garde::Validate;
use serde::Deserialize;
use springtale_store::paths;

/// Top-level daemon configuration.
///
/// Loaded from `springtale.toml` with env var overrides (SPRINGTALE_ prefix).
/// Validated with garde after deserialization (architecture doc §8.1).
#[derive(Debug, Deserialize, Validate)]
pub struct SpringtaleConfig {
    /// Ephemeral mode: all state in memory, lost on exit.
    /// WARNING: No persistence. Useful for travel mode, demos, privacy-critical terminals.
    #[serde(default)]
    #[garde(skip)]
    pub ephemeral: bool,
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
    /// Heartbeat interval in seconds (default 1800 = 30 minutes).
    /// Set to 0 to disable heartbeat.
    #[serde(default = "default_heartbeat_interval")]
    #[garde(skip)]
    pub heartbeat_interval_secs: u64,
    /// Bot runtime configuration. If absent, bot is disabled.
    #[serde(default)]
    #[garde(skip)]
    pub bot: Option<springtale_bot::BotConfig>,
    /// Telegram connector configuration. If absent, connector not loaded.
    #[serde(default)]
    #[garde(skip)]
    pub telegram: Option<connector_telegram::TelegramConfig>,
    /// Sentinel behavioral monitor configuration. If absent, uses defaults.
    #[serde(default)]
    #[garde(skip)]
    pub sentinel: Option<springtale_sentinel::SentinelConfig>,
    /// Ollama AI adapter. If absent, not used.
    #[serde(default)]
    #[garde(skip)]
    pub ai_ollama: Option<springtale_ai::OllamaConfig>,
    /// OpenAI-compatible AI adapter. If absent, not used.
    #[serde(default)]
    #[garde(skip)]
    pub ai_openai: Option<springtale_ai::OpenAiConfig>,
    /// Anthropic AI adapter. If absent, not used.
    #[serde(default)]
    #[garde(skip)]
    pub ai_anthropic: Option<springtale_ai::AnthropicConfig>,
    /// Nostr connector configuration. If absent, connector not loaded.
    #[serde(default)]
    #[garde(skip)]
    pub nostr: Option<connector_nostr::NostrConfig>,
    /// IRC connector configuration. If absent, connector not loaded.
    #[serde(default)]
    #[garde(skip)]
    pub irc: Option<connector_irc::IrcConfig>,
    /// Discord connector configuration. If absent, connector not loaded.
    /// WARNING: Discord complies with government data requests.
    #[serde(default)]
    #[garde(skip)]
    pub discord: Option<connector_discord::DiscordConfig>,
    /// Slack connector configuration. If absent, connector not loaded.
    /// WARNING: Workspace admins can read ALL messages including DMs.
    #[serde(default)]
    #[garde(skip)]
    pub slack: Option<connector_slack::SlackConfig>,
    /// Signal connector configuration. If absent, connector not loaded.
    /// Bridges to signal-cli daemon for E2E encrypted messaging.
    #[serde(default)]
    #[garde(skip)]
    pub signal: Option<connector_signal::SignalConfig>,
    /// Bluesky connector configuration. If absent, connector not loaded.
    /// Subscribes to the Jetstream firehose for own-post / mention triggers.
    #[serde(default)]
    #[garde(skip)]
    pub bluesky: Option<connector_bluesky::BlueskyConfig>,
    // connector-matrix: DEFERRED — matrix-sdk 0.16 requires rusqlite 0.37
    // which has CVE-2025-70873. Waiting for matrix-sdk to update.
}

#[derive(Debug, Deserialize, Validate)]
pub struct StoreConfig {
    /// Path to the SQLite database file.
    #[serde(default = "paths::default_db_path")]
    #[garde(custom(validate_path))]
    pub path: PathBuf,

    /// Days to retain events and audit logs. None = keep forever.
    #[serde(default)]
    #[garde(skip)]
    pub retention_days: Option<u32>,
}

impl Default for StoreConfig {
    fn default() -> Self {
        Self {
            path: paths::default_db_path(),
            retention_days: None,
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
    /// Transport type: "local" (default) or "http".
    #[serde(default = "default_transport_type")]
    #[garde(skip)]
    pub transport_type: String,

    /// Path to the Unix domain socket (for local transport).
    #[serde(default = "paths::default_socket_path")]
    #[garde(custom(validate_path))]
    pub socket_path: PathBuf,

    /// HTTP transport configuration (required when transport_type = "http").
    #[serde(default)]
    #[garde(skip)]
    pub http: Option<springtale_transport::http::HttpTransportConfig>,
}

fn default_transport_type() -> String {
    "local".to_owned()
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            transport_type: default_transport_type(),
            socket_path: paths::default_socket_path(),
            http: None,
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

fn default_heartbeat_interval() -> u64 {
    1800 // 30 minutes
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
/// Returns the typed config and raw connector config values (for factory
/// instantiation without requiring Serialize on Secret-bearing configs).
///
/// After deserialization, validates with garde per architecture doc §8.1.
pub fn load_config() -> Result<LoadedConfig, anyhow::Error> {
    let figment = Figment::new()
        .merge(Toml::file("springtale.toml"))
        // Use __ (double underscore) as nesting separator to preserve
        // single underscores in field names like vault_path, socket_path.
        // Example: SPRINGTALE_CRYPTO__VAULT_PATH=/path sets crypto.vault_path
        .merge(Env::prefixed("SPRINGTALE_").map(|key| key.as_str().replace("__", ".").into()));

    let config: SpringtaleConfig = figment.extract()?;

    // Validate configuration (garde)
    config.validate()?;

    let connector_configs = extract_connector_configs(&figment);

    Ok(LoadedConfig {
        config,
        connector_configs,
    })
}

/// Result of loading configuration.
pub struct LoadedConfig {
    pub config: SpringtaleConfig,
    /// Raw connector configs keyed by config_key (e.g., "telegram").
    /// Extracted as raw JSON values to avoid Serialize on Secret-bearing types.
    pub connector_configs: std::collections::HashMap<String, serde_json::Value>,
}

/// Extract connector configuration sections as raw JSON values.
///
/// Each connector factory declares a `config_key()` (e.g., "telegram").
/// We extract that key from the Figment source as `serde_json::Value`,
/// preserving raw strings for Secret fields.
fn extract_connector_configs(
    figment: &Figment,
) -> std::collections::HashMap<String, serde_json::Value> {
    let keys = [
        "telegram",
        "nostr",
        "irc",
        "discord",
        "slack",
        "signal",
        "github",
        "kick",
        "presearch",
        "bluesky",
        "http",
        "filesystem",
        "shell",
        "browser",
    ];
    let mut configs = std::collections::HashMap::new();
    for key in keys {
        if let Ok(val) = figment.extract_inner::<serde_json::Value>(key) {
            configs.insert(key.to_string(), val);
        }
    }
    configs
}
