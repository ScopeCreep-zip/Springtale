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
    }
}
