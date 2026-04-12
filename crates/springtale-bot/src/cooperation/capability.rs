//! Dynamic capability binding — agent capabilities change at runtime.
//!
//! Per COOPERATION.pdf §16:
//! Game source: It Takes Two chapter-based reassignment.
//!
//! "Every chapter gives both players completely new tools. Hammer + nails.
//! Sap gun + match gun. Size asymmetry. Time manipulation. The cooperative
//! STRUCTURE persists (interdependent tools that chain together) while
//! the specific TOOLS change entirely."

/// An agent's complete capability set — layered from multiple sources.
///
/// From COOPERATION.pdf §16:
/// ```text
/// pub struct DynamicCapabilitySet {
///     pub base_capabilities: Vec<CapabilityDecl>,       // connector manifest
///     pub context_capabilities: Vec<CapabilityDecl>,    // formation context
///     pub momentum_unlocked: Vec<CapabilityDecl>,       // momentum tier
///     pub transformed_capabilities: Vec<CapabilityDecl>, // role transformation
/// }
/// ```
pub struct DynamicCapabilitySet {
    /// Base capabilities from connector manifest.
    pub base_capabilities: Vec<String>,
    /// Capabilities granted by formation context.
    pub context_capabilities: Vec<String>,
    /// Capabilities unlocked by momentum tier (§7).
    pub momentum_unlocked: Vec<String>,
    /// Capabilities from role transformation (§14).
    pub transformed_capabilities: Vec<String>,
}

impl DynamicCapabilitySet {
    /// Get all currently available capabilities (union of all layers).
    pub fn all(&self) -> Vec<&str> {
        self.base_capabilities
            .iter()
            .chain(self.context_capabilities.iter())
            .chain(self.momentum_unlocked.iter())
            .chain(self.transformed_capabilities.iter())
            .map(|s| s.as_str())
            .collect()
    }

    /// Check if a capability is available from any layer.
    pub fn has(&self, cap: &str) -> bool {
        self.all().contains(&cap)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_layered_capabilities() {
        let set = DynamicCapabilitySet {
            base_capabilities: vec!["slack_send".into()],
            context_capabilities: vec!["formation_read".into()],
            momentum_unlocked: vec!["ai_call".into()],
            transformed_capabilities: vec![],
        };
        assert!(set.has("slack_send"));
        assert!(set.has("ai_call"));
        assert!(!set.has("nuclear_launch"));
        assert_eq!(set.all().len(), 3);
    }
}
