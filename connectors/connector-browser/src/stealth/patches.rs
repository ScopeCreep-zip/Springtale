//! Inline JS patches for the "Minimal" stealth profile.
//!
//! Each patch is a stand-alone `const &str` so the test suite can
//! assert specific properties (e.g. "navigator.webdriver patch
//! deletes the property, not assigns false") and so we can grow the
//! patch set without touching launch logic.
//!
//! The patches concatenate into one script delivered to
//! `Page::AddScriptToEvaluateOnNewDocument` so all three apply
//! atomically before any page script runs.

/// Launch flags applied when `StealthProfile::Minimal` is active.
/// `--disable-blink-features=AutomationControlled` is the
/// Chromium-side complement to the JS `navigator.webdriver` patch —
/// some bot detectors check both surfaces.
pub const MINIMAL_LAUNCH_FLAGS: &[&str] = &["--disable-blink-features=AutomationControlled"];

/// Patch 1: `navigator.webdriver = undefined`.
///
/// Real Chrome doesn't expose `webdriver` on `navigator`. Headless
/// Chrome exposes it as `true`. The naive evasion sets it to `false`,
/// which is itself a tell-tale: real browsers don't have the property
/// at all. We `delete` the descriptor so `'webdriver' in navigator`
/// returns `false` (matching real Chrome behavior).
const PATCH_NAVIGATOR_WEBDRIVER: &str = r#"
(() => {
  try {
    // Some Chrome versions surface `webdriver` via the prototype;
    // remove there too.
    const proto = Object.getPrototypeOf(navigator);
    if (proto && Object.getOwnPropertyDescriptor(proto, 'webdriver')) {
      delete proto.webdriver;
    }
    if (Object.getOwnPropertyDescriptor(navigator, 'webdriver')) {
      delete navigator.webdriver;
    }
    // Defensive re-definition: if anything later tries to set it
    // (e.g. devtools polyfills), keep returning undefined.
    Object.defineProperty(navigator, 'webdriver', {
      get: () => undefined,
      configurable: true,
    });
  } catch (_) { /* silent — patch is best-effort */ }
})();
"#;

/// Patch 2: Strip `HeadlessChrome` from `navigator.userAgent`.
///
/// Paired with `--disable-blink-features=AutomationControlled`
/// at launch. This patch is the JS-side belt to the launch-flag
/// braces — some detectors fingerprint the UA from a Worker context
/// where the launch-flag-driven UA override doesn't always
/// propagate. `userAgentData.brands` doesn't carry "HeadlessChrome"
/// in current Chrome, so no patch needed there.
const PATCH_USER_AGENT: &str = r#"
(() => {
  try {
    const ua = navigator.userAgent;
    if (ua && ua.includes('HeadlessChrome')) {
      const cleaned = ua.replace(/HeadlessChrome/g, 'Chrome');
      Object.defineProperty(navigator, 'userAgent', {
        get: () => cleaned,
        configurable: true,
      });
      Object.defineProperty(navigator, 'appVersion', {
        get: () => cleaned.replace(/^Mozilla\/[\d.]+ /, ''),
        configurable: true,
      });
    }
  } catch (_) { /* silent */ }
})();
"#;

/// Patch 3: Complete `window.chrome` with `loadTimes()` / `csi()`.
///
/// Real Chrome ships these methods. Headless Chrome ships the bare
/// `chrome` object without them. The shape of these methods is
/// well-documented; stable stub values are sufficient for the
/// fingerprint-completeness check most detectors do.
const PATCH_WINDOW_CHROME: &str = r#"
(() => {
  try {
    if (typeof window.chrome === 'undefined' || window.chrome === null) {
      Object.defineProperty(window, 'chrome', {
        value: {},
        writable: true,
        enumerable: true,
        configurable: true,
      });
    }
    if (typeof window.chrome.loadTimes !== 'function') {
      window.chrome.loadTimes = function () {
        // Stable stub. Real Chrome returns navigation timing
        // values; detectors only check that the method exists and
        // returns an object with the expected keys.
        return {
          commitLoadTime: 0,
          connectionInfo: 'http/1.1',
          finishDocumentLoadTime: 0,
          finishLoadTime: 0,
          firstPaintAfterLoadTime: 0,
          firstPaintTime: 0,
          navigationType: 'Other',
          npnNegotiatedProtocol: 'unknown',
          requestTime: 0,
          startLoadTime: 0,
          wasAlternateProtocolAvailable: false,
          wasFetchedViaSpdy: false,
          wasNpnNegotiated: false,
        };
      };
    }
    if (typeof window.chrome.csi !== 'function') {
      window.chrome.csi = function () {
        return {
          onloadT: 0,
          pageT: 0,
          startE: 0,
          tran: 15,
        };
      };
    }
  } catch (_) { /* silent */ }
})();
"#;

/// The concatenated patch script delivered to
/// `Page::AddScriptToEvaluateOnNewDocument`. All three patches run
/// in one IIFE-per-patch sequence before any page script.
pub fn minimal_patch_script() -> String {
    let mut out = String::with_capacity(
        PATCH_NAVIGATOR_WEBDRIVER.len() + PATCH_USER_AGENT.len() + PATCH_WINDOW_CHROME.len() + 64,
    );
    out.push_str("// Springtale stealth: Minimal profile (3 patches)\n");
    out.push_str(PATCH_NAVIGATOR_WEBDRIVER);
    out.push_str(PATCH_USER_AGENT);
    out.push_str(PATCH_WINDOW_CHROME);
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn navigator_webdriver_patch_deletes_property_not_assigns_false() {
        // The patched value `false` is itself a stealth tell. We
        // delete the property + define a getter that returns
        // undefined. Lock that in with a literal-substring check
        // so future edits can't silently regress.
        assert!(PATCH_NAVIGATOR_WEBDRIVER.contains("delete navigator.webdriver"));
        assert!(PATCH_NAVIGATOR_WEBDRIVER.contains("get: () => undefined"));
        assert!(!PATCH_NAVIGATOR_WEBDRIVER.contains("= false"));
    }

    #[test]
    fn user_agent_patch_strips_headless_chrome_only() {
        // Don't blanket-rewrite the UA — only target the
        // `HeadlessChrome` substring. Keeps the platform / Chrome
        // version intact.
        assert!(PATCH_USER_AGENT.contains("HeadlessChrome"));
        assert!(PATCH_USER_AGENT.contains("ua.replace"));
    }

    #[test]
    fn window_chrome_patch_adds_loadtimes_and_csi() {
        assert!(PATCH_WINDOW_CHROME.contains("loadTimes"));
        assert!(PATCH_WINDOW_CHROME.contains("csi"));
        // Defensive guards: don't overwrite real Chrome's methods
        // when they already exist (running against a non-headless
        // browser, etc.).
        assert!(PATCH_WINDOW_CHROME.contains("typeof window.chrome.loadTimes !== 'function'"));
    }

    #[test]
    fn minimal_patch_script_concatenates_all_three() {
        let script = minimal_patch_script();
        assert!(script.contains("navigator.webdriver"));
        assert!(script.contains("HeadlessChrome"));
        assert!(script.contains("loadTimes"));
        // Sanity check the assembled size is in the expected range.
        assert!(script.len() > 1000, "stealth script too small");
        assert!(script.len() < 8000, "stealth script too large");
    }

    #[test]
    fn launch_flags_include_automation_controlled_disable() {
        assert!(
            MINIMAL_LAUNCH_FLAGS
                .iter()
                .any(|f| f.contains("disable-blink-features=AutomationControlled")),
            "must pair JS patch with Chromium launch flag"
        );
    }
}
