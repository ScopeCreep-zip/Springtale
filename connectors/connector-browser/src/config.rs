use serde::Deserialize;

/// Browser connector configuration.
///
/// Headless Chromium automation with domain allow-list.
/// Each allowed domain becomes a Capability::NetworkOutbound entry
/// in the connector manifest — cannot navigate to unapproved sites.
///
/// Chrome/Chromium must be pre-installed on the system.
/// Chrome telemetry is disabled by default for privacy.
#[derive(Deserialize)]
pub struct BrowserConfig {
    /// Allowed domains — only these can be navigated to.
    /// Each domain becomes a Capability::NetworkOutbound in the manifest.
    /// No wildcards (*.example.com not allowed per security rules).
    pub allowed_domains: Vec<String>,

    /// Path to Chrome/Chromium binary. Auto-detected if not set.
    #[serde(default)]
    pub chrome_path: Option<String>,

    /// Disable Chrome telemetry (default: true).
    #[serde(default = "default_true")]
    pub disable_telemetry: bool,

    /// Publish-side jitter in seconds (0 = disabled).
    #[serde(default)]
    pub message_jitter_secs: u64,

    /// Stealth-patch policy. Default `Off` — Springtale never markets
    /// anti-bot bypass as a feature (`feedback_no_ban_risk`).
    /// `Minimal` applies three high-signal patches at
    /// `Page::evaluate_on_new_document` time to avoid incidental
    /// headless-detection on sites with reflexive blocks. See
    /// `crate::stealth` for the patch set + rationale.
    #[serde(default)]
    pub stealth_profile: StealthProfile,
}

/// Stealth-patch policy for [`BrowserConfig`]. `Off` is the default.
///
/// `Minimal` applies three patches per
/// [DataDome's 2026 detection research][dd] + the
/// [Playwright stealth retrospective][stealth-2026]:
///   1. `navigator.webdriver = undefined` (NOT `false` — detectors
///      test for the patched-value tell).
///   2. `--disable-blink-features=AutomationControlled` launch flag
///      + `Network.setUserAgentOverride` to strip `HeadlessChrome`
///      from the UA string.
///   3. `window.chrome` completion with `loadTimes()` / `csi()`.
///
/// We explicitly skip `iframe.contentWindow` (DataDome detects the
/// evasion's internal code; also crashes some sites' DOM), bad
/// `navigator.plugins` faking (fails `instanceof PluginArray`), and
/// WebGL vendor spoofing (creates a new fingerprint without a
/// matching OS/hardware story).
///
/// [dd]: https://datadome.co/threat-research/how-datadome-detects-puppeteer-extra-stealth/
/// [stealth-2026]: https://dev.to/vhub_systems_ed5641f65d59/playwright-stealth-mode-in-2026-the-7-patches-that-actually-matter-46bp
#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StealthProfile {
    /// No patches. Default — Springtale isn't an anti-bot-bypass tool.
    #[default]
    Off,
    /// Three high-signal patches. Opt-in per-connector via config.
    Minimal,
}

impl StealthProfile {
    pub fn is_enabled(self) -> bool {
        !matches!(self, StealthProfile::Off)
    }
}

fn default_true() -> bool {
    true
}

impl std::fmt::Debug for BrowserConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BrowserConfig")
            .field("allowed_domains", &self.allowed_domains)
            .field("chrome_path", &self.chrome_path)
            .field("disable_telemetry", &self.disable_telemetry)
            .field("message_jitter_secs", &self.message_jitter_secs)
            .field("stealth_profile", &self.stealth_profile)
            .finish()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let config: BrowserConfig = serde_json::from_value(serde_json::json!({
            "allowed_domains": ["example.com"]
        }))
        .unwrap();

        assert!(config.disable_telemetry);
        assert_eq!(config.message_jitter_secs, 0);
        assert!(config.chrome_path.is_none());
        // Stealth defaults to Off — Springtale never opts in by default.
        assert_eq!(config.stealth_profile, StealthProfile::Off);
        assert!(!config.stealth_profile.is_enabled());
    }

    #[test]
    fn test_stealth_minimal_parses() {
        let config: BrowserConfig = serde_json::from_value(serde_json::json!({
            "allowed_domains": ["example.com"],
            "stealth_profile": "minimal",
        }))
        .unwrap();
        assert_eq!(config.stealth_profile, StealthProfile::Minimal);
        assert!(config.stealth_profile.is_enabled());
    }

    #[test]
    fn test_stealth_off_explicit_parses() {
        let config: BrowserConfig = serde_json::from_value(serde_json::json!({
            "allowed_domains": ["example.com"],
            "stealth_profile": "off",
        }))
        .unwrap();
        assert_eq!(config.stealth_profile, StealthProfile::Off);
    }
}
