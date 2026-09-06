//! Runtime configuration — shared between springtaled and desktop.
//!
//! springtaled's SpringtaleConfig extends this with daemon-specific
//! fields (api bind, transport, connector configs, heartbeat interval).
//! Desktop uses this directly.

use specta::Type;
use std::collections::HashMap;
use std::path::PathBuf;

use serde::Deserialize;

/// Shared runtime configuration.
#[derive(Debug, Default, Deserialize, Type)]
pub struct RuntimeConfig {
    /// Store configuration.
    #[serde(default)]
    pub store: StoreConfig,

    /// Sentinel behavioral monitor config (optional).
    #[serde(default)]
    pub sentinel: Option<springtale_sentinel::SentinelConfig>,

    /// Connector configurations keyed by config_key (e.g., "telegram", "github").
    /// Each value is the raw JSON for that connector's config section.
    /// Populated by the app layer (springtaled extracts from Figment,
    /// desktop extracts from UI config).
    #[serde(default)]
    pub connector_configs: HashMap<String, serde_json::Value>,

    /// Cooperation-layer runtime config — how formations gossip across
    /// processes (spec §8). Default is single-process with an in-memory
    /// gossip store. Enabling `cross_process` spawns a chitchat node
    /// that joins `chitchat_seeds`.
    #[serde(default)]
    pub cooperation: CooperationConfig,
}

/// Cooperation-layer runtime configuration.
#[derive(Debug, Clone, Default, Deserialize, Type)]
pub struct CooperationConfig {
    /// Gossip substrate selection. `false` (default) uses the
    /// in-process `InMemoryGossipStore` (`DashMap`, zero network). `true`
    /// spawns a `ChitchatGossipStore` over UDP loopback so multiple
    /// springtaled processes on the same machine share one gossip view.
    #[serde(default)]
    pub cross_process: bool,

    /// Local `host:port` the chitchat node binds + advertises. Ignored
    /// when `cross_process = false`.
    #[serde(default)]
    pub chitchat_listen_addr: Option<String>,

    /// Chitchat seed nodes (`host:port`) this process should try to
    /// reach at startup. Ignored when `cross_process = false`.
    #[serde(default)]
    pub chitchat_seeds: Vec<String>,

    /// Plan §1.15 G: utterance def table. Defaults to the built-in table;
    /// a `[cooperation.utterances.<name>]` block replaces that one def
    /// (all fields required — see `springtale.toml.example`).
    #[serde(default)]
    pub utterances: springtale_cooperation::utterance::UtteranceDefs,

    /// Chitchat cluster identifier. Two nodes with different cluster
    /// ids won't peer with each other. Ignored when `cross_process = false`.
    #[serde(default = "default_cluster_id")]
    pub cluster_id: String,

    /// Local `host:port` the SWIM liveness node binds. Ignored when
    /// `cross_process = false`. If unset, picks an ephemeral port on
    /// loopback (127.0.0.1:0).
    #[serde(default)]
    pub swim_listen_addr: Option<String>,

    /// SWIM seed nodes (`host:port`). The local SWIM node announces to
    /// each seed at startup. Ignored when `cross_process = false`.
    #[serde(default)]
    pub swim_seeds: Vec<String>,
}

fn default_cluster_id() -> String {
    "springtale".to_owned()
}

// Default derived — all fields have sensible defaults
// (StoreConfig::default(), None, HashMap::new()).

/// Store configuration.
#[derive(Debug, Clone, Deserialize, Type)]
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
