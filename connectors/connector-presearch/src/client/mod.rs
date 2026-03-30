use async_trait::async_trait;
use secrecy::{ExposeSecret, SecretBox};

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
        let inner = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| {
                PresearchError::QueryFailed(format!("failed to build client: {e}"))
            })?;

        Ok(Self {
            inner,
            api_base: config.api_base.clone(),
            // SECURITY: key stays wrapped, exposed only at HTTP call site
            api_key: SecretBox::new(Box::new(config.api_key.expose_secret().clone())),
            allowed_scrape_hosts: config.all_network_hosts(),
        })
    }

    /// Execute a search query against the Presearch API (internal implementation).
    async fn do_search(&self, query: &str) -> Result<serde_json::Value, PresearchError> {
        let url = format!("{}/search?q={}", self.api_base, urlencoded(query));

        let response = self
            .inner
            .get(&url)
            // SECURITY: expose needed for API key header
            .header("Presearch-Key", self.api_key.expose_secret().as_str())
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

        serde_json::from_str(&body).map_err(|e| {
            PresearchError::QueryFailed(format!("failed to parse response: {e}"))
        })
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

/// Simple percent-encoding for query parameters.
fn urlencoded(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => result.push(c),
            ' ' => result.push('+'),
            _ => {
                for byte in c.to_string().as_bytes() {
                    result.push('%');
                    result.push_str(&format!("{byte:02X}"));
                }
            }
        }
    }
    result
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
            cache_ttl_secs: 300, allowed_scrape_hosts: vec![],
        };
        let client = PresearchClient::new(&config);
        assert!(client.is_ok());
    }

    #[test]
    fn test_urlencoded() {
        assert_eq!(urlencoded("hello world"), "hello+world");
        assert_eq!(urlencoded("rust+lang"), "rust%2Blang");
        assert_eq!(urlencoded("simple"), "simple");
    }
}
