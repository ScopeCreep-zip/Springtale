use secrecy::SecretBox;
use serde::Deserialize;
use springtale_connector::config::deserialize_secret;

/// Default TTL for cached results in seconds (5 minutes).
const DEFAULT_CACHE_TTL_SECS: u64 = 300;

/// Default API base URL.
const DEFAULT_API_BASE: &str = "https://presearch.com";

/// Configuration for the Presearch connector.
#[derive(Deserialize)]
pub struct PresearchConfig {
    /// Presearch API key.
    #[serde(deserialize_with = "deserialize_secret")]
    pub api_key: SecretBox<String>,

    /// API base URL. Default: `https://presearch.com`.
    #[serde(default = "default_api_base")]
    pub api_base: String,

    /// Cache TTL in seconds. Cached search results are reused within this window.
    /// Default: 300 (5 minutes).
    #[serde(default = "default_cache_ttl")]
    pub cache_ttl_secs: u64,

    /// Hosts the scrape action is allowed to fetch from.
    /// Each host becomes a `NetworkOutbound` capability in the manifest.
    /// If empty, scrape only works for the API base host.
    #[serde(default)]
    pub allowed_scrape_hosts: Vec<String>,
}

fn default_api_base() -> String {
    DEFAULT_API_BASE.to_owned()
}

fn default_cache_ttl() -> u64 {
    DEFAULT_CACHE_TTL_SECS
}

impl std::fmt::Debug for PresearchConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PresearchConfig")
            .field("api_key", &"[REDACTED]")
            .field("api_base", &self.api_base)
            .field("cache_ttl_secs", &self.cache_ttl_secs)
            .finish()
    }
}

impl PresearchConfig {
    /// Get the cache TTL as a Duration.
    pub fn cache_ttl(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.cache_ttl_secs)
    }

    /// Check whether a host is allowed for scraping.
    /// Always allows the API base host. Additional hosts from config.
    pub fn is_scrape_host_allowed(&self, host: &str) -> bool {
        // Always allow the API base host
        if let Some(base_host) = extract_host(&self.api_base)
            && host == base_host
        {
            return true;
        }
        self.allowed_scrape_hosts.iter().any(|h| h == host)
    }

    /// Get all hosts that need NetworkOutbound capabilities.
    pub fn all_network_hosts(&self) -> Vec<String> {
        let mut hosts = Vec::new();
        if let Some(base_host) = extract_host(&self.api_base) {
            hosts.push(base_host.to_owned());
        }
        for h in &self.allowed_scrape_hosts {
            if !hosts.contains(h) {
                hosts.push(h.clone());
            }
        }
        hosts
    }
}

/// Extract host from a URL string.
fn extract_host(url: &str) -> Option<&str> {
    url.strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .and_then(|rest| rest.split('/').next())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_debug_redacts_secret() {
        let config = PresearchConfig {
            api_key: SecretBox::new(Box::new("secret_key".to_owned())),
            api_base: default_api_base(),
            cache_ttl_secs: DEFAULT_CACHE_TTL_SECS,
            allowed_scrape_hosts: vec![],
        };
        let debug = format!("{config:?}");
        assert!(!debug.contains("secret_key"));
        assert!(debug.contains("[REDACTED]"));
    }

    #[test]
    fn test_cache_ttl_default() {
        let config = PresearchConfig {
            api_key: SecretBox::new(Box::new("key".to_owned())),
            api_base: default_api_base(),
            cache_ttl_secs: DEFAULT_CACHE_TTL_SECS,
            allowed_scrape_hosts: vec![],
        };
        assert_eq!(config.cache_ttl(), std::time::Duration::from_secs(300));
    }
}
