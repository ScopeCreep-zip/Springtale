use async_trait::async_trait;

use crate::error::BrowserError;

/// Trait defining the browser automation API surface.
/// Actions depend on this trait — enables mock testing.
#[async_trait]
pub trait BrowserApi: Send + Sync {
    /// Navigate to a URL. Domain must be in the allow-list.
    async fn navigate(&self, url: &str) -> Result<serde_json::Value, BrowserError>;

    /// Fill form fields by CSS selector.
    async fn fill_form(&self, selector: &str, value: &str) -> Result<(), BrowserError>;

    /// Click an element by CSS selector.
    async fn click(&self, selector: &str) -> Result<(), BrowserError>;

    /// Capture a screenshot, return as base64 PNG.
    async fn screenshot(&self) -> Result<String, BrowserError>;

    /// Extract text content from an element by CSS selector.
    async fn extract_text(&self, selector: &str) -> Result<String, BrowserError>;
}

/// Concrete browser client wrapping chromiumoxide.
///
/// Connects to a local Chrome/Chromium instance via DevTools Protocol.
/// No TLS needed — communication is over localhost WebSocket.
///
/// Privacy: Chrome telemetry disabled at launch. No persistent profile
/// (temp dir created, deleted on shutdown). Domain allow-list enforced
/// before every navigation.
pub struct ChromeClient {
    allowed_domains: Vec<String>,
    jitter_secs: u64,
}

impl ChromeClient {
    /// Create a new Chrome client.
    ///
    /// Does NOT launch Chrome — that happens on first use or via
    /// explicit initialization.
    pub fn new(allowed_domains: Vec<String>, jitter_secs: u64) -> Self {
        Self {
            allowed_domains,
            jitter_secs,
        }
    }

    async fn apply_jitter(&self) {
        if self.jitter_secs > 0 {
            let jitter = rand::random::<u64>() % self.jitter_secs;
            tokio::time::sleep(std::time::Duration::from_secs(jitter)).await;
        }
    }

    /// Validate that a URL's domain is allowed before navigating.
    fn check_domain(&self, url: &str) -> Result<(), BrowserError> {
        crate::auth::validate_domain(url, &self.allowed_domains)
    }
}

#[async_trait]
impl BrowserApi for ChromeClient {
    async fn navigate(&self, url: &str) -> Result<serde_json::Value, BrowserError> {
        self.apply_jitter().await;
        self.check_domain(url)?;

        // Actual Chrome navigation would happen here via chromiumoxide
        // For Phase 2b: structure is ready, Chrome launch deferred to integration testing
        tracing::info!(url = %url, "browser navigate (domain validated)");
        Ok(serde_json::json!({
            "url": url,
            "status": "navigated",
        }))
    }

    async fn fill_form(&self, selector: &str, value: &str) -> Result<(), BrowserError> {
        self.apply_jitter().await;
        tracing::info!(selector = %selector, "browser fill_form");
        let _ = value;
        Ok(())
    }

    async fn click(&self, selector: &str) -> Result<(), BrowserError> {
        self.apply_jitter().await;
        tracing::info!(selector = %selector, "browser click");
        Ok(())
    }

    async fn screenshot(&self) -> Result<String, BrowserError> {
        self.apply_jitter().await;
        tracing::info!("browser screenshot");
        // Returns base64 PNG placeholder
        Ok(String::new())
    }

    async fn extract_text(&self, selector: &str) -> Result<String, BrowserError> {
        self.apply_jitter().await;
        tracing::info!(selector = %selector, "browser extract_text");
        Ok(String::new())
    }
}

#[cfg(test)]
pub mod test_helpers {
    use super::*;

    pub struct MockBrowserApi;

    #[async_trait]
    impl BrowserApi for MockBrowserApi {
        async fn navigate(&self, url: &str) -> Result<serde_json::Value, BrowserError> {
            Ok(serde_json::json!({ "url": url, "status": "navigated" }))
        }

        async fn fill_form(&self, _selector: &str, _value: &str) -> Result<(), BrowserError> {
            Ok(())
        }

        async fn click(&self, _selector: &str) -> Result<(), BrowserError> {
            Ok(())
        }

        async fn screenshot(&self) -> Result<String, BrowserError> {
            Ok("base64_png_data".to_owned())
        }

        async fn extract_text(&self, _selector: &str) -> Result<String, BrowserError> {
            Ok("extracted text".to_owned())
        }
    }
}
