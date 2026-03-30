use std::collections::HashMap;

use serde::Deserialize;

/// Default request timeout in seconds.
const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Configuration for the HTTP connector.
///
/// Deserialized from TOML config. Never serialized.
#[derive(Debug, Clone, Deserialize)]
pub struct HttpConfig {
    /// Hosts that the connector is allowed to make requests to.
    /// Exact host match — no wildcards.
    /// If empty, no requests can be made.
    #[serde(default)]
    pub allowed_hosts: Vec<String>,

    /// Default headers to include in every request.
    /// Useful for setting User-Agent, Accept, etc.
    #[serde(default)]
    pub default_headers: HashMap<String, String>,

    /// Request timeout in seconds. Default: 30s.
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
}

fn default_timeout_secs() -> u64 {
    DEFAULT_TIMEOUT_SECS
}

impl HttpConfig {
    /// Get the timeout as a `Duration`.
    pub fn timeout_duration(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.timeout_secs)
    }

    /// Check whether a host is in the allow-list.
    pub fn is_host_allowed(&self, host: &str) -> bool {
        self.allowed_hosts.iter().any(|h| h == host)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_host_allowed() {
        let config = HttpConfig {
            allowed_hosts: vec!["api.example.com".to_owned(), "data.example.com".to_owned()],
            default_headers: HashMap::new(),
            timeout_secs: 30,
        };

        assert!(config.is_host_allowed("api.example.com"));
        assert!(config.is_host_allowed("data.example.com"));
        assert!(!config.is_host_allowed("evil.com"));
        assert!(!config.is_host_allowed(""));
    }

    #[test]
    fn test_empty_allowlist_blocks_all() {
        let config = HttpConfig {
            allowed_hosts: vec![],
            default_headers: HashMap::new(),
            timeout_secs: 30,
        };

        assert!(!config.is_host_allowed("anything.com"));
    }

    #[test]
    fn test_default_timeout() {
        let config = HttpConfig {
            allowed_hosts: vec![],
            default_headers: HashMap::new(),
            timeout_secs: DEFAULT_TIMEOUT_SECS,
        };

        assert_eq!(
            config.timeout_duration(),
            std::time::Duration::from_secs(30)
        );
    }
}
