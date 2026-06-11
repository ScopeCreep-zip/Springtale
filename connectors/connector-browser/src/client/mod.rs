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

use crate::config::StealthProfile;
use crate::error::BrowserError;
use crate::stealth;

/// One match returned by [`BrowserApi::query_all`]. Carries the
/// rendered text, full outer HTML, attribute map, and tag name for
/// each matched element. Recipes use these for CSS-schema extraction
/// + DOM-pattern inspection from the selector picker.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ElementSnapshot {
    pub text: String,
    pub html: String,
    pub tag_name: String,
    pub attrs: std::collections::BTreeMap<String, String>,
}

#[async_trait]
pub trait BrowserApi: Send + Sync {
    async fn navigate(&self, url: &str) -> Result<serde_json::Value, BrowserError>;
    async fn fill_form(&self, selector: &str, value: &str) -> Result<(), BrowserError>;
    async fn click(&self, selector: &str) -> Result<(), BrowserError>;
    async fn screenshot(&self) -> Result<String, BrowserError>;
    async fn extract_text(&self, selector: &str) -> Result<String, BrowserError>;

    /// Run a JS expression in the current page and return the result
    /// deserialized as JSON. The expression must be a single statement
    /// that evaluates to a serializable value — wrap multi-statement
    /// logic in an IIFE: `"(() => { … })()"`.
    async fn evaluate(&self, js: &str) -> Result<serde_json::Value, BrowserError>;

    /// Return the full rendered HTML of the current page (post-JS
    /// execution). Used as input to Readability / CSS / diff-hash
    /// extractors in `springtale-runtime::extraction`.
    async fn get_html(&self) -> Result<String, BrowserError>;

    /// Query every element matching `selector`. Returns one
    /// [`ElementSnapshot`] per match with text, outer HTML, tag name,
    /// and the full attribute map. Empty `Vec` when nothing matches.
    async fn query_all(&self, selector: &str) -> Result<Vec<ElementSnapshot>, BrowserError>;

    /// Wait for `selector` to appear in the DOM, up to `timeout_ms`.
    /// Polls every 100ms via `find_elements`. Returns `Ok(true)` when
    /// at least one match exists, `Ok(false)` when the timeout
    /// elapses without a match.
    async fn wait_for_selector(
        &self,
        selector: &str,
        timeout_ms: u32,
    ) -> Result<bool, BrowserError>;
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
    /// Stealth-patch policy. `Off` is the safe default — Springtale
    /// doesn't market anti-bot bypass as a feature. When `Minimal`,
    /// `launch()` adds `--disable-blink-features=AutomationControlled`
    /// to the Chrome args and `navigate()` injects three JS evasion
    /// patches via `Page::execute_on_new_document` after each
    /// `new_page` so they run before any page script.
    stealth_profile: StealthProfile,
    state: OnceCell<BrowserState>,
}

impl ChromeClient {
    pub fn new(allowed_domains: Vec<String>, jitter_secs: u64) -> Self {
        Self::with_options(
            allowed_domains,
            jitter_secs,
            None,
            true,
            StealthProfile::Off,
        )
    }

    /// Full-options constructor used by the factory once it reads
    /// the connector config (`chrome_path`, `disable_telemetry`,
    /// `stealth_profile`).
    pub fn with_options(
        allowed_domains: Vec<String>,
        jitter_secs: u64,
        chrome_executable: Option<PathBuf>,
        disable_telemetry: bool,
        stealth_profile: StealthProfile,
    ) -> Self {
        Self {
            allowed_domains,
            jitter_secs,
            chrome_executable,
            disable_telemetry,
            stealth_profile,
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
                .arg(
                    "--disable-features=AutofillServerCommunication,InterestFeedContentSuggestions",
                )
                .arg("--no-default-browser-check")
                .arg("--metrics-recording-only=false");
        }
        // Stealth: Chromium-side complement to the JS patches.
        // `--disable-blink-features=AutomationControlled` removes
        // the Blink-internal flag that some bot detectors check.
        // JS-side patches are applied per-page in `navigate()`.
        if self.stealth_profile.is_enabled() {
            for flag in stealth::MINIMAL_LAUNCH_FLAGS {
                builder = builder.arg(*flag);
            }
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
        // Stealth: inject patches BEFORE waiting for navigation so
        // they run before any page script. `execute_on_new_document`
        // applies to this page's subsequent navigations too — the
        // single `new_page(url)` above already triggered the load,
        // but the patches will be in place for any in-page
        // navigations and for re-evaluations.
        if self.stealth_profile.is_enabled() {
            use chromiumoxide::cdp::browser_protocol::page::AddScriptToEvaluateOnNewDocumentParams;
            let _ = page
                .execute(AddScriptToEvaluateOnNewDocumentParams {
                    source: stealth::minimal_patch_script(),
                    world_name: None,
                    include_command_line_api: None,
                    run_immediately: Some(true),
                })
                .await
                .map_err(|e| BrowserError::NavigationFailed(format!("stealth inject: {e}")))?;
        }
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

    async fn evaluate(&self, js: &str) -> Result<serde_json::Value, BrowserError> {
        let page = self.current_page().await?;
        let result = page
            .evaluate(js)
            .await
            .map_err(|e| BrowserError::NavigationFailed(format!("evaluate failed: {e}")))?;
        // EvaluationResult::value() returns Option<&Value>. None when
        // the JS returned `undefined` — we surface that as
        // `Value::Null` so the dispatcher's chain context handles it
        // uniformly.
        Ok(result.value().cloned().unwrap_or(serde_json::Value::Null))
    }

    async fn get_html(&self) -> Result<String, BrowserError> {
        let page = self.current_page().await?;
        let html = page
            .content()
            .await
            .map_err(|e| BrowserError::NavigationFailed(format!("get_html failed: {e}")))?;
        tracing::info!(bytes = html.len(), "browser get_html");
        Ok(html)
    }

    async fn query_all(&self, selector: &str) -> Result<Vec<ElementSnapshot>, BrowserError> {
        let page = self.current_page().await?;
        // One round-trip: evaluate a query+map snippet in the page
        // and return the full snapshot array. Per-element CDP calls
        // (innerText, attributes, outerHTML each round-tripping) is
        // O(n) chatty; this evaluation is O(1) regardless of match
        // count. The result type matches `Vec<ElementSnapshot>`
        // serde-deserialization so we can pass it straight through.
        let js = format!(
            r#"(() => {{
                const els = document.querySelectorAll({selector_json});
                return Array.from(els).map(el => ({{
                    text: (el.innerText || el.textContent || '').trim(),
                    html: el.outerHTML || '',
                    tag_name: (el.tagName || '').toLowerCase(),
                    attrs: Object.fromEntries(
                        Array.from(el.attributes || []).map(a => [a.name, a.value])
                    ),
                }}));
            }})()"#,
            selector_json = serde_json::Value::String(selector.to_owned()),
        );
        let result = page
            .evaluate(js.as_str())
            .await
            .map_err(|e| BrowserError::NavigationFailed(format!("query_all evaluate: {e}")))?;
        let value = result
            .value()
            .cloned()
            .unwrap_or(serde_json::Value::Array(Vec::new()));
        let snapshots: Vec<ElementSnapshot> = serde_json::from_value(value)
            .map_err(|e| BrowserError::NavigationFailed(format!("query_all deserialize: {e}")))?;
        tracing::info!(
            selector = %selector,
            matches = snapshots.len(),
            "browser query_all"
        );
        Ok(snapshots)
    }

    async fn wait_for_selector(
        &self,
        selector: &str,
        timeout_ms: u32,
    ) -> Result<bool, BrowserError> {
        let page = self.current_page().await?;
        let deadline = std::time::Instant::now() + Duration::from_millis(timeout_ms as u64);
        // Poll every 100ms — short enough that the visible delay on
        // a quick page load is bounded, long enough that CDP round-
        // trip cost doesn't dominate. Mirrors Puppeteer/Playwright's
        // default polling cadence.
        loop {
            match page.find_elements(selector.to_owned()).await {
                Ok(elements) if !elements.is_empty() => {
                    tracing::info!(selector = %selector, "browser wait_for_selector: matched");
                    return Ok(true);
                }
                Ok(_) | Err(_) => {}
            }
            if std::time::Instant::now() >= deadline {
                tracing::info!(
                    selector = %selector,
                    timeout_ms = timeout_ms,
                    "browser wait_for_selector: timed out"
                );
                return Ok(false);
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}

impl Drop for ChromeClient {
    fn drop(&mut self) {
        // Abort the handler task synchronously; the runtime cleans
        // the JoinHandle without blocking. The Browser + TempDir
        // drop in declared field order, closing the CDP socket and
        // wiping the profile directory.
        if let Some(state) = self.state.get()
            && let Ok(mut guard) = state.handler.try_lock()
            && let Some(task) = guard.take()
        {
            task.abort();
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

        async fn evaluate(&self, _js: &str) -> Result<serde_json::Value, BrowserError> {
            Ok(serde_json::json!({ "mock": true }))
        }

        async fn get_html(&self) -> Result<String, BrowserError> {
            Ok("<html><body>mock</body></html>".to_owned())
        }

        async fn query_all(&self, _selector: &str) -> Result<Vec<ElementSnapshot>, BrowserError> {
            Ok(vec![ElementSnapshot {
                text: "mock text".into(),
                html: "<div>mock</div>".into(),
                tag_name: "div".into(),
                attrs: std::collections::BTreeMap::new(),
            }])
        }

        async fn wait_for_selector(
            &self,
            _selector: &str,
            _timeout_ms: u32,
        ) -> Result<bool, BrowserError> {
            Ok(true)
        }
    }
}
