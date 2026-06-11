use async_trait::async_trait;
use secrecy::SecretBox;
use springtale_connector::encoding::urlencoded;

use crate::config::PresearchConfig;
use crate::error::PresearchError;

/// Trait defining the Presearch API surface.
///
/// Actions depend on this trait, not the concrete client. This enables
/// mock implementations in tests (per testing.md: "mock at the client
/// layer, not at reqwest level").
#[async_trait]
pub trait PresearchApi: Send + Sync {
    async fn search(&self, query: &str) -> Result<serde_json::Value, PresearchError>;
    async fn fetch_url(&self, url: &str) -> Result<String, PresearchError>;
}

/// Presearch API client.
///
/// All network calls to Presearch go through this client. The API key
/// is sent as a header for authentication.
pub struct PresearchClient {
    inner: reqwest::Client,
    api_base: String,
    api_key: SecretBox<String>,
    /// Hosts allowed for scrape requests (from config).
    allowed_scrape_hosts: Vec<String>,
}

impl PresearchClient {
    /// Create a new Presearch API client from config.
    pub fn new(config: &PresearchConfig) -> Result<Self, PresearchError> {
        let inner = springtale_transport::safe_http::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| PresearchError::QueryFailed(format!("failed to build client: {e}")))?;

        Ok(Self {
            inner,
            api_base: config.api_base.clone(),
            api_key: springtale_crypto::secret_use::clone_into_box(&config.api_key),
            allowed_scrape_hosts: config.all_network_hosts(),
        })
    }

    /// Execute a search query against the Presearch API (internal implementation).
    async fn do_search(&self, query: &str) -> Result<serde_json::Value, PresearchError> {
        let url = format!("{}/search?q={}", self.api_base, urlencoded(query, true));

        let response = self
            .inner
            .get(&url)
            .header(
                "Presearch-Key",
                springtale_crypto::secret_use::header_value(&self.api_key),
            )
            .header("User-Agent", "Springtale")
            .header("Accept", "application/json")
            .send()
            .await?;

        let status = response.status().as_u16();
        let body = response
            .text()
            .await
            .map_err(|e| PresearchError::QueryFailed(format!("failed to read response: {e}")))?;

        if status >= 400 {
            return Err(PresearchError::QueryFailed(format!(
                "Presearch API returned {status}: {body}"
            )));
        }

        serde_json::from_str(&body)
            .map_err(|e| PresearchError::QueryFailed(format!("failed to parse response: {e}")))
    }

    /// Fetch the content of a URL (internal implementation).
    ///
    /// Validates the URL scheme (https/http only) and host against the
    /// allowed scrape hosts list. Prevents SSRF to internal services.
    async fn do_fetch_url(&self, url: &str) -> Result<String, PresearchError> {
        // Validate URL scheme — only http/https allowed
        if !url.starts_with("https://") && !url.starts_with("http://") {
            return Err(PresearchError::QueryFailed(format!(
                "invalid URL scheme (only http/https allowed): {url}"
            )));
        }

        // Validate host against allowed list
        let host = url
            .strip_prefix("https://")
            .or_else(|| url.strip_prefix("http://"))
            .and_then(|rest| rest.split('/').next())
            .ok_or_else(|| {
                PresearchError::QueryFailed(format!("could not parse host from URL: {url}"))
            })?;

        if !self.allowed_scrape_hosts.iter().any(|h| h == host) {
            return Err(PresearchError::QueryFailed(format!(
                "host not in allowed scrape list: {host}"
            )));
        }

        let response = self
            .inner
            .get(url)
            .header("User-Agent", "Springtale")
            .send()
            .await?;

        let status = response.status().as_u16();
        let body = response
            .text()
            .await
            .map_err(|e| PresearchError::QueryFailed(format!("failed to read page: {e}")))?;

        if status >= 400 {
            return Err(PresearchError::QueryFailed(format!(
                "fetch returned {status} for {url}"
            )));
        }

        Ok(body)
    }
}

#[async_trait]
impl PresearchApi for PresearchClient {
    async fn search(&self, query: &str) -> Result<serde_json::Value, PresearchError> {
        self.do_search(query).await
    }

    async fn fetch_url(&self, url: &str) -> Result<String, PresearchError> {
        self.do_fetch_url(url).await
    }
}

#[cfg(test)]
pub mod test_helpers {
    use super::*;

    /// Configurable mock for `PresearchApi`.
    ///
    /// Set `search_response` for search tests and `fetch_response` for
    /// scrape tests. Unused fields can be left as defaults.
    pub struct MockPresearchClient {
        pub search_response: serde_json::Value,
        pub fetch_response: String,
    }

    impl MockPresearchClient {
        /// Create a mock configured for search tests.
        pub fn for_search(response: serde_json::Value) -> Self {
            Self {
                search_response: response,
                fetch_response: String::new(),
            }
        }

        /// Create a mock configured for fetch/scrape tests.
        pub fn for_fetch(response: String) -> Self {
            Self {
                search_response: serde_json::json!({}),
                fetch_response: response,
            }
        }
    }

    #[async_trait]
    impl PresearchApi for MockPresearchClient {
        async fn search(&self, _query: &str) -> Result<serde_json::Value, PresearchError> {
            Ok(self.search_response.clone())
        }

        async fn fetch_url(&self, _url: &str) -> Result<String, PresearchError> {
            Ok(self.fetch_response.clone())
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use secrecy::SecretBox;

    #[test]
    fn test_client_creation() {
        let config = PresearchConfig {
            api_key: SecretBox::new(Box::new("test_key".to_owned())),
            api_base: "https://presearch.com".to_owned(),
            cache_ttl_secs: 300,
            allowed_scrape_hosts: vec![],
        };
        let client = PresearchClient::new(&config);
        assert!(client.is_ok());
    }

    #[test]
    fn test_urlencoded() {
        assert_eq!(urlencoded("hello world", true), "hello+world");
        assert_eq!(urlencoded("rust+lang", true), "rust%2Blang");
        assert_eq!(urlencoded("simple", true), "simple");
    }
}
