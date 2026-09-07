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
    // Chat connectors are NOT typed fields here (plan 6.4): every
    // `[connectors.telegram]` (or bare `[telegram]`) table is picked up
    // verbatim by `extract_connector_configs`, for whatever connectors
    // are actually installed, and installed through the same
    // `setup_connector` path a runtime install takes — so the daemon
    // holds no per-connector knowledge. The bot's own persona / context
    // window / tool policy are runtime settings (plan 6.3), not config.
    /// Sentinel behavioral monitor configuration. If absent, uses defaults.
    #[serde(default)]
    #[garde(skip)]
    pub sentinel: Option<springtale_sentinel::SentinelConfig>,
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
///
/// Which keys to look for comes from the compile-time factory registry
/// (`springtale_connector::factory::config_keys`), not from a list
/// written here. The list used to be written here, so a connector added
/// after it was written could not be configured from the file at all —
/// its table was read by nobody, silently. A connector that is installed
/// is now configurable, by construction.
///
/// Two table shapes are accepted per connector, the namespaced one
/// winning when both are present:
///
/// ```toml
/// [connectors.telegram]   # namespaced — cannot collide with daemon config
/// bot_token = "..."
///
/// [telegram]              # bare — the historical shape, still read
/// bot_token = "..."
/// ```
fn extract_connector_configs(
    figment: &Figment,
) -> std::collections::HashMap<String, serde_json::Value> {
    let mut configs = std::collections::HashMap::new();
    for key in springtale_connector::factory::config_keys() {
        // Namespaced first: an explicit `[connectors.x]` is unambiguous,
        // so it wins over a bare `[x]` table of the same name.
        if let Ok(val) = figment.extract_inner::<serde_json::Value>(&format!("connectors.{key}")) {
            configs.insert(key.to_owned(), val);
            continue;
        }
        if let Ok(val) = figment.extract_inner::<serde_json::Value>(key) {
            configs.insert(key.to_owned(), val);
        }
    }
    configs
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use figment::providers::Format;

    /// The connector config keys the daemon hard-coded before the list
    /// came from the registry. Every one has to keep resolving: a config
    /// file that worked must not go quietly unread. If a connector ever
    /// renames its `config_key`, this fails — map the old name to the new
    /// one here rather than dropping it.
    const HISTORICAL_KEYS: [&str; 14] = [
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

    fn figment_from(toml: &str) -> Figment {
        Figment::new().merge(Toml::string(toml))
    }

    #[test]
    fn test_registry_keys_cover_every_historical_key() {
        let keys = springtale_connector::factory::config_keys();
        for key in HISTORICAL_KEYS {
            assert!(
                keys.contains(&key),
                "config key '{key}' no longer resolves to an installed connector — \
                 add an alias so existing config files keep working"
            );
        }
    }

    #[test]
    fn test_extract_connector_configs_reads_every_historical_key() {
        let toml: String = HISTORICAL_KEYS
            .iter()
            .map(|k| format!("[{k}]\nprobe = \"set\"\n"))
            .collect();
        let configs = extract_connector_configs(&figment_from(&toml));
        for key in HISTORICAL_KEYS {
            assert_eq!(
                configs.get(key).and_then(|v| v.get("probe")),
                Some(&serde_json::Value::String("set".to_owned())),
                "historical key '{key}' stopped being extracted"
            );
        }
    }

    /// The point of the change: a connector outside the fourteen the
    /// daemon used to know is configurable from the file.
    #[test]
    fn test_extract_connector_configs_reads_keys_beyond_the_historical_list() {
        let beyond: Vec<&'static str> = springtale_connector::factory::config_keys()
            .into_iter()
            .filter(|k| !HISTORICAL_KEYS.contains(k))
            .collect();
        assert!(
            !beyond.is_empty(),
            "no compiled-in connector outside the fourteen hard-coded keys — \
             this test needs one to mean anything"
        );
        for key in beyond {
            let configs =
                extract_connector_configs(&figment_from(&format!("[{key}]\nprobe = \"set\"\n")));
            assert!(
                configs.contains_key(key),
                "connector '{key}' is installed but its config table was ignored"
            );
        }
    }

    #[test]
    fn test_extract_connector_configs_reads_namespaced_table() {
        let configs = extract_connector_configs(&figment_from(
            "[connectors.telegram]\nbot_token = \"namespaced\"\n",
        ));
        assert_eq!(
            configs["telegram"]["bot_token"],
            serde_json::Value::String("namespaced".to_owned())
        );
    }

    #[test]
    fn test_extract_connector_configs_namespaced_table_wins() {
        let configs = extract_connector_configs(&figment_from(
            "[telegram]\nbot_token = \"bare\"\n\n[connectors.telegram]\nbot_token = \"namespaced\"\n",
        ));
        assert_eq!(
            configs["telegram"]["bot_token"],
            serde_json::Value::String("namespaced".to_owned())
        );
    }

    #[test]
    fn test_extract_connector_configs_ignores_unknown_table() {
        let configs =
            extract_connector_configs(&figment_from("[not_a_connector]\nprobe = \"set\"\n"));
        assert!(!configs.contains_key("not_a_connector"));
    }
}
