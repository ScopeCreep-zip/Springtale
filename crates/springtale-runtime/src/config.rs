//! Runtime configuration — shared between springtaled and desktop.
//!
//! springtaled's SpringtaleConfig extends this with daemon-specific
//! fields (api bind, transport, connector configs, heartbeat interval).
//! Desktop uses this directly.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::Deserialize;

/// Shared runtime configuration.
#[derive(Debug, Default, Deserialize)]
pub struct RuntimeConfig {
    /// Store configuration.
    #[serde(default)]
    pub store: StoreConfig,

    /// Ollama AI adapter config (optional).
    #[serde(default)]
    pub ai_ollama: Option<springtale_ai::OllamaConfig>,

    /// OpenAI-compatible adapter config (optional).
    #[serde(default)]
    pub ai_openai: Option<springtale_ai::OpenAiConfig>,

    /// Anthropic adapter config (optional).
    #[serde(default)]
    pub ai_anthropic: Option<springtale_ai::AnthropicConfig>,

    /// Sentinel behavioral monitor config (optional).
    #[serde(default)]
    pub sentinel: Option<springtale_sentinel::SentinelConfig>,

    /// Connector configurations keyed by config_key (e.g., "telegram", "github").
    /// Each value is the raw JSON for that connector's config section.
    /// Populated by the app layer (springtaled extracts from Figment,
    /// desktop extracts from UI config).
    #[serde(default)]
    pub connector_configs: HashMap<String, serde_json::Value>,
}

// Default derived — all fields have sensible defaults
// (StoreConfig::default(), None, HashMap::new()).

/// Store configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct StoreConfig {
    /// Path to the SQLite database.
    #[serde(default = "default_store_path")]
    pub path: PathBuf,

    /// Use in-memory backend (lost on exit).
    #[serde(default)]
    pub ephemeral: bool,

    /// Hex-encoded 32-byte encryption key for SQLite encryption at rest.
    /// When set, the database is encrypted with ChaCha20-Poly1305 via
    /// SQLite3MultipleCiphers. Derived from vault passphrase.
    #[serde(default)]
    pub encryption_key_hex: Option<String>,

    /// Days to retain events and audit logs. None = keep forever.
    /// When set, a background task purges expired data hourly.
    #[serde(default)]
    pub retention_days: Option<u32>,
}

impl Default for StoreConfig {
    fn default() -> Self {
        Self {
            path: default_store_path(),
            ephemeral: false,
            encryption_key_hex: None,
            retention_days: None,
        }
    }
}

fn default_store_path() -> PathBuf {
    springtale_store::paths::default_db_path()
}
