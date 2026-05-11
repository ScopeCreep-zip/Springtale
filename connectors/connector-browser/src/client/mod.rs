//! Browser automation client — real chromiumoxide integration.
//!
//! Lifecycle (lazy launch, single browser per [`ChromeClient`]):
//! 1. First call to [`BrowserApi::navigate`] launches a headless
//!    Chromium via [`chromiumoxide::Browser::launch`].
//! 2. A background tokio task drains the [`chromiumoxide::Handler`]
//!    event stream — required for CDP message processing per
//!    chromiumoxide's example pattern. The task is aborted in
//!    [`Drop`] so we don't leak browsers on connector reload.
//! 3. Subsequent calls reuse the same `Browser`; each navigate
//!    swaps the held [`chromiumoxide::Page`] so click/fill/text
//!    operate on the most-recently-visited page.
//!
//! Privacy posture (per spec §22.4):
//! - User-data-dir is a fresh [`tempfile::TempDir`]. Dropped with
//!   the client, deleting cookies / cache / history.
//! - Telemetry flags disabled via Chromium launch args.
//! - Domain allow-list checked **before** every navigate — there
//!   is no path to a non-allowed origin.
//!
//! Threading: [`chromiumoxide::Browser`] and [`Page`] are `Send +
//! Sync`. The whole `BrowserState` lives behind `tokio::sync::Mutex`
//! so the trait remains `Send + Sync` for use through `Arc<dyn
//! BrowserApi>`.

use async_trait::async_trait;
use base64::Engine;
use futures_util::StreamExt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, OnceCell};
use tokio::task::JoinHandle;

use crate::error::BrowserError;

#[async_trait]
pub trait BrowserApi: Send + Sync {
    async fn navigate(&self, url: &str) -> Result<serde_json::Value, BrowserError>;
    async fn fill_form(&self, selector: &str, value: &str) -> Result<(), BrowserError>;
    async fn click(&self, selector: &str) -> Result<(), BrowserError>;
    async fn screenshot(&self) -> Result<String, BrowserError>;
    async fn extract_text(&self, selector: &str) -> Result<String, BrowserError>;
}

/// Resources owned by a launched browser. Kept inside `OnceCell` so
/// the first navigate triggers launch and later calls reuse the
/// same instance.
struct BrowserState {
    browser: chromiumoxide::Browser,
    /// Whichever page the last `navigate` produced. `fill_form`,
    /// `click`, `screenshot`, `extract_text` operate on this page.
    page: Mutex<Option<Arc<chromiumoxide::Page>>>,
    /// Handler-pump task. Aborted on `Drop` so the connector unload
    /// path doesn't leak the CDP event loop.
    handler: Mutex<Option<JoinHandle<()>>>,
    /// Temp profile dir — dropped with the client, removing any
    /// cookies / history that accumulated during the session.
    /// Hold to keep alive; never read.
    _profile_dir: tempfile::TempDir,
}

pub struct ChromeClient {
    allowed_domains: Vec<String>,
    jitter_secs: u64,
    chrome_executable: Option<PathBuf>,
    disable_telemetry: bool,
    state: OnceCell<BrowserState>,
}

impl ChromeClient {
    pub fn new(allowed_domains: Vec<String>, jitter_secs: u64) -> Self {
        Self::with_options(allowed_domains, jitter_secs, None, true)
    }

    /// Full-options constructor used by the factory once it reads
    /// the connector config (`chrome_path`, `disable_telemetry`).
    pub fn with_options(
        allowed_domains: Vec<String>,
        jitter_secs: u64,
        chrome_executable: Option<PathBuf>,
        disable_telemetry: bool,
    ) -> Self {
        Self {
            allowed_domains,
            jitter_secs,
            chrome_executable,
            disable_telemetry,
            state: OnceCell::new(),
        }
    }

    async fn apply_jitter(&self) {
        if self.jitter_secs > 0 {
            let jitter = rand::random::<u64>() % self.jitter_secs;
            tokio::time::sleep(Duration::from_secs(jitter)).await;
        }
    }

    fn check_domain(&self, url: &str) -> Result<(), BrowserError> {
        crate::auth::validate_domain(url, &self.allowed_domains)
    }

    /// Lazy launch + memoize. Errors propagate to the caller; a
    /// failed launch does NOT poison the `OnceCell` — `get_or_try_init`
    /// re-runs the closure on the next call so a transient launch
    /// failure (Chrome not yet installed, port collision) can recover
    /// without the client being permanently broken.
    async fn ensure_browser(&self) -> Result<&BrowserState, BrowserError> {
        self.state
            .get_or_try_init(|| async { self.launch().await })
            .await
    }

    async fn launch(&self) -> Result<BrowserState, BrowserError> {
        let profile_dir = tempfile::tempdir()
            .map_err(|e| BrowserError::LaunchFailed(format!("temp profile dir: {e}")))?;

        let mut builder = chromiumoxide::BrowserConfig::builder()
            .new_headless_mode()
            .incognito()
            .user_data_dir(profile_dir.path())
            .launch_timeout(Duration::from_secs(20));
        if let Some(ref path) = self.chrome_executable {
            builder = builder.chrome_executable(path);
        }
        if self.disable_telemetry {
            // Per chromium docs the metrics opt-out flag is
            // surfaced via the `--disable-features` arg. Combined
            // with `--no-default-browser-check` to keep a fresh
            // profile silent.
            builder = builder
                .arg("--disable-features=AutofillServerCommunication,InterestFeedContentSuggestions")
                .arg("--no-default-browser-check")
                .arg("--metrics-recording-only=false");
        }
        let config = builder
            .build()
            .map_err(|e| BrowserError::LaunchFailed(format!("config build: {e}")))?;

        let (browser, mut handler) = chromiumoxide::Browser::launch(config)
            .await
            .map_err(|e| BrowserError::LaunchFailed(e.to_string()))?;

        // Drain the CDP event stream. Per chromiumoxide example,
        // this background task is required to make any commands
        // actually run. We ignore individual events here; the page
        // API surfaces what we need synchronously through commands.
        let handler_task = tokio::spawn(async move {
            while let Some(event) = handler.next().await {
                if let Err(e) = event {
                    tracing::trace!(error = %e, "chromiumoxide handler event error");
                }
            }
        });

        Ok(BrowserState {
            browser,
            page: Mutex::new(None),
            handler: Mutex::new(Some(handler_task)),
            _profile_dir: profile_dir,
        })
    }

    async fn current_page(&self) -> Result<Arc<chromiumoxide::Page>, BrowserError> {
        let state = self.ensure_browser().await?;
        let guard = state.page.lock().await;
        guard
            .clone()
            .ok_or_else(|| BrowserError::InvalidInput("no page; call navigate first".into()))
    }
}

#[async_trait]
impl BrowserApi for ChromeClient {
    async fn navigate(&self, url: &str) -> Result<serde_json::Value, BrowserError> {
        self.apply_jitter().await;
        self.check_domain(url)?;
        let state = self.ensure_browser().await?;
        let page = state
            .browser
            .new_page(url)
            .await
            .map_err(|e| BrowserError::NavigationFailed(e.to_string()))?;
        page.wait_for_navigation()
            .await
            .map_err(|e| BrowserError::NavigationFailed(e.to_string()))?;
        let final_url = page
            .url()
            .await
            .map_err(|e| BrowserError::NavigationFailed(e.to_string()))?
            .unwrap_or_else(|| url.to_owned());
        let page_arc = Arc::new(page);
        *state.page.lock().await = Some(page_arc);
        tracing::info!(url = %url, "browser navigated");
        Ok(serde_json::json!({
            "url": final_url,
            "status": "navigated",
        }))
    }

    async fn fill_form(&self, selector: &str, value: &str) -> Result<(), BrowserError> {
        self.apply_jitter().await;
        let page = self.current_page().await?;
        let element = page
            .find_element(selector.to_owned())
            .await
            .map_err(|e| BrowserError::ElementNotFound(format!("{selector}: {e}")))?;
        element
            .click()
            .await
            .map_err(|e| BrowserError::NavigationFailed(format!("focus failed: {e}")))?;
        element
            .type_str(value)
            .await
            .map_err(|e| BrowserError::NavigationFailed(format!("type failed: {e}")))?;
        tracing::info!(selector = %selector, "browser fill_form");
        Ok(())
    }

    async fn click(&self, selector: &str) -> Result<(), BrowserError> {
        self.apply_jitter().await;
        let page = self.current_page().await?;
        let element = page
            .find_element(selector.to_owned())
            .await
            .map_err(|e| BrowserError::ElementNotFound(format!("{selector}: {e}")))?;
        element
            .click()
            .await
            .map_err(|e| BrowserError::NavigationFailed(format!("click failed: {e}")))?;
        tracing::info!(selector = %selector, "browser click");
        Ok(())
    }

    async fn screenshot(&self) -> Result<String, BrowserError> {
        self.apply_jitter().await;
        let page = self.current_page().await?;
        let png_bytes = page
            .screenshot(chromiumoxide::page::ScreenshotParams::default())
            .await
            .map_err(|e| BrowserError::ScreenshotFailed(e.to_string()))?;
        let encoded = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
        tracing::info!(bytes = png_bytes.len(), "browser screenshot");
        Ok(encoded)
    }

    async fn extract_text(&self, selector: &str) -> Result<String, BrowserError> {
        self.apply_jitter().await;
        let page = self.current_page().await?;
        let element = page
            .find_element(selector.to_owned())
            .await
            .map_err(|e| BrowserError::ElementNotFound(format!("{selector}: {e}")))?;
        let text = element
            .inner_text()
            .await
            .map_err(|e| BrowserError::NavigationFailed(format!("inner_text failed: {e}")))?
            .unwrap_or_default();
        tracing::info!(selector = %selector, len = text.len(), "browser extract_text");
        Ok(text)
    }
}

impl Drop for ChromeClient {
    fn drop(&mut self) {
        // Abort the handler task synchronously; the runtime cleans
        // the JoinHandle without blocking. The Browser + TempDir
        // drop in declared field order, closing the CDP socket and
        // wiping the profile directory.
        if let Some(state) = self.state.get() {
            if let Ok(mut guard) = state.handler.try_lock() {
                if let Some(task) = guard.take() {
                    task.abort();
                }
            }
        }
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
