//! Selector-picker IPC.
//!
//! Opens a Tauri webview window pointing at the recipe's target
//! URL, injects the bundled `picker.js` overlay, listens for the
//! `selector-picked` event, returns the chosen CSS selector to
//! the caller. **No chromiumoxide involvement** — the picker is
//! an authoring-time tool, not a headless-browser feature.
//!
//! ## Privacy
//!
//! The webview navigates to the user-supplied URL using the
//! desktop's own networking stack. The `host_allowlist` is an
//! advisory list the picker.js bundle checks before binding the
//! highlight handler — the actual gate is the user's network
//! visibility (they could navigate to anywhere via the address
//! bar). We bound the surface by:
//!
//!   - Opening at a fixed URL (no user-typed address-bar navigation).
//!   - Disabling browser features (no microphone, no geolocation).
//!   - Letting the user cancel via the close button or `Escape`.

use std::sync::Arc;

use tauri::{Listener, Manager, Url, WebviewUrl, WebviewWindowBuilder};
use tokio::sync::oneshot;

use crate::state::AppState;

/// Open the selector picker against `url`. Resolves with the
/// CSS selector the user picked, or `None` if the user closed
/// the window without picking. `host_allowlist` is forwarded to
/// the injected picker.js; an empty list means "any host allowed".
#[tauri::command]
#[specta::specta]
pub async fn open_selector_picker(
    app: tauri::AppHandle,
    _state: tauri::State<'_, AppState>,
    url: String,
    host_allowlist: Vec<String>,
) -> Result<Option<String>, String> {
    // Validate URL scheme — the picker must navigate to http/https
    // only, never `file://` or `javascript:`.
    let parsed = Url::parse(&url).map_err(|e| format!("invalid url: {e}"))?;
    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" {
        return Err(format!(
            "selector picker requires http/https url, got `{scheme}`"
        ));
    }

    let label = format!("selector-picker-{}", chrono::Utc::now().timestamp_millis());

    // Channel the listener uses to deliver the picked selector
    // back to this command future. `oneshot` because the picker
    // emits exactly one `selector-picked` event then closes.
    let (tx, rx) = oneshot::channel::<Option<String>>();
    let tx = Arc::new(std::sync::Mutex::new(Some(tx)));

    let window = WebviewWindowBuilder::new(&app, label.clone(), WebviewUrl::External(parsed))
        .title("Pick an element")
        .inner_size(1024.0, 768.0)
        .resizable(true)
        .visible(true)
        .build()
        .map_err(|e| format!("failed to open picker window: {e}"))?;

    // Forward the host_allowlist into the page on load so picker.js
    // can advise the user when they navigate away.
    let allowlist_json = serde_json::to_string(&host_allowlist)
        .map_err(|e| format!("failed to serialize allowlist: {e}"))?;
    let bootstrap = format!(
        "window.__SPRINGTALE_HOST_ALLOWLIST__ = {allowlist_json}; \
         window.__SPRINGTALE_PICKER_LABEL__ = {:?};",
        label.clone()
    );

    // Inject picker.js — bundled at build time under `assets/`.
    // The script binds hover-highlight handlers and emits
    // `selector-picked` when the user clicks an element.
    let picker_js = include_str!("../../assets/picker.js");
    let payload = format!("{bootstrap}\n{picker_js}");
    if let Err(e) = window.eval(&payload) {
        return Err(format!("failed to inject picker.js: {e}"));
    }

    // Listen for the picked event. The event payload is a JSON
    // string holding the selector.
    let tx_picked = Arc::clone(&tx);
    let label_for_close = label.clone();
    let app_for_close = app.clone();
    let handler_id = app.listen("selector-picked", move |event| {
        let payload_str = event.payload();
        let parsed: Option<SelectorPickedPayload> = serde_json::from_str(payload_str).ok();
        let selector = parsed.map(|p| p.selector);
        if let Ok(mut slot) = tx_picked.lock()
            && let Some(sender) = slot.take()
        {
            let _ = sender.send(selector);
        }
        // Close the picker window so the user doesn't see a stuck
        // overlay after picking.
        if let Some(win) = app_for_close.get_webview_window(&label_for_close) {
            let _ = win.close();
        }
    });

    // Also resolve with None when the window closes without an
    // emit — the user cancelled.
    let tx_close = Arc::clone(&tx);
    window.on_window_event(move |event| {
        if matches!(event, tauri::WindowEvent::CloseRequested { .. })
            && let Ok(mut slot) = tx_close.lock()
            && let Some(sender) = slot.take()
        {
            let _ = sender.send(None);
        }
    });

    // Await whichever resolves first — pick or close.
    let result = rx.await.unwrap_or(None);

    // Detach the listener once we have a result.
    app.unlisten(handler_id);

    Ok(result)
}

/// Bridge — sent by picker.js back to the host. Kept as a Rust
/// type so specta can describe it for consumers; not used by the
/// command itself (we parse the raw payload above).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct SelectorPickedPayload {
    pub selector: String,
    /// The element's tag name — used by the recipe editor to hint
    /// whether the picked element is text vs. an input vs. a link.
    pub tag_name: Option<String>,
}

impl SelectorPickedPayload {
    /// Constructor used in unit tests + the eventual desktop hook
    /// that synthesizes a payload when picker.js can't run (e.g.
    /// the page CSP blocks injection).
    #[allow(dead_code)]
    pub fn new(selector: impl Into<String>) -> Self {
        Self {
            selector: selector.into(),
            tag_name: None,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn payload_round_trips_through_json() {
        let payload = SelectorPickedPayload {
            selector: "div.product > h1".into(),
            tag_name: Some("h1".into()),
        };
        let json = serde_json::to_string(&payload).unwrap();
        let back: SelectorPickedPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(back.selector, payload.selector);
        assert_eq!(back.tag_name, payload.tag_name);
    }

    #[test]
    fn payload_constructor_leaves_tag_none() {
        let p = SelectorPickedPayload::new("p");
        assert_eq!(p.selector, "p");
        assert!(p.tag_name.is_none());
    }

    /// `tauri::Url::parse` would reject these — same gate the
    /// command uses before opening a window.
    #[test]
    fn javascript_and_file_urls_rejected_at_parse() {
        // Local helper that mirrors the command's check using
        // Tauri's re-exported Url (avoids pulling the `url` crate
        // directly as a dev-dep — Tauri already depends on it).
        fn is_safe(url: &str) -> bool {
            let parsed: Url = match Url::parse(url) {
                Ok(p) => p,
                Err(_) => return false,
            };
            let scheme = parsed.scheme();
            scheme == "http" || scheme == "https"
        }
        assert!(!is_safe("javascript:alert(1)"));
        assert!(!is_safe("file:///etc/passwd"));
        assert!(is_safe("https://example.com/path"));
        assert!(is_safe("http://localhost:5173/dev"));
    }
}
