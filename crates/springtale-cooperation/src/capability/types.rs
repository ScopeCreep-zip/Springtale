use crate::capability::CapabilityDecl;
use crate::momentum::MomentumTier;

/// Layered capability set — base + context + momentum-unlocked + transformed.
///
/// Layers are merged by `all()` / `has()` / effective-capability projection
/// in `binder`. Keeping them separate preserves provenance: a capability
/// gained via transformation can be stripped without touching the base
/// connector-manifest list.
pub struct DynamicCapabilitySet {
    pub base_capabilities: Vec<CapabilityDecl>,
    pub context_capabilities: Vec<CapabilityDecl>,
    pub momentum_unlocked: Vec<CapabilityDecl>,
    pub transformed_capabilities: Vec<CapabilityDecl>,
}

impl DynamicCapabilitySet {
    /// Return a union view across all four layers.
    pub fn all(&self) -> Vec<&CapabilityDecl> {
        self.base_capabilities
            .iter()
            .chain(self.context_capabilities.iter())
            .chain(self.momentum_unlocked.iter())
            .chain(self.transformed_capabilities.iter())
            .collect()
    }

    pub fn has(&self, cap: &str) -> bool {
        self.all().iter().any(|c| **c == *cap)
    }

    /// Rebind the `momentum_unlocked` layer for the given tier.
    ///
    /// Logic lives in `binder::unlocked_for_tier` so the tier → strings map
    /// has exactly one source-of-truth.
    pub fn rebind_for_tier(&mut self, tier: MomentumTier) {
        self.momentum_unlocked = super::binder::unlocked_for_tier(tier)
            .iter()
            .map(|s| CapabilityDecl::new(*s))
            .collect();
    }

    /// Construct a new set from base capabilities and bind the momentum
    /// layer for `tier`.
    pub fn new_for_tier(base: Vec<CapabilityDecl>, tier: MomentumTier) -> Self {
        let mut set = Self {
            base_capabilities: base,
            context_capabilities: vec![],
            momentum_unlocked: vec![],
            transformed_capabilities: vec![],
        };
        set.rebind_for_tier(tier);
        set
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn rebind_cold_keeps_only_read_env() {
        let set = DynamicCapabilitySet::new_for_tier(vec!["slack".into()], MomentumTier::Cold);
        assert!(set.has("read_env"));
        assert!(!set.has("read_neighbors"));
        assert!(!set.has("chain"));
        assert!(!set.has("ai_call"));
    }

    #[test]
    fn rebind_warming_unlocks_neighbors_and_chain() {
        let set = DynamicCapabilitySet::new_for_tier(vec!["slack".into()], MomentumTier::Warming);
        assert!(set.has("read_env"));
        assert!(set.has("read_neighbors"));
        assert!(set.has("chain"));
        assert!(!set.has("write_env"));
    }

    #[test]
    fn rebind_hot_unlocks_write_env_and_commit() {
        let set = DynamicCapabilitySet::new_for_tier(vec!["slack".into()], MomentumTier::Hot);
        assert!(set.has("write_env"));
        assert!(set.has("synchronized_commit"));
        assert!(!set.has("consensus"));
    }

    #[test]
    fn rebind_fever_unlocks_consensus_ai_recruit() {
        let set = DynamicCapabilitySet::new_for_tier(vec!["slack".into()], MomentumTier::Fever);
        assert!(set.has("consensus"));
        assert!(set.has("ai_call"));
        assert!(set.has("recruit"));
        assert!(set.has("slack"));
    }

    #[test]
    fn rebind_preserves_base_capabilities() {
        let mut set = DynamicCapabilitySet::new_for_tier(
            vec!["github".into(), "slack".into()],
            MomentumTier::Fever,
        );
        set.rebind_for_tier(MomentumTier::Cold);
        assert!(set.has("github"));
        assert!(set.has("slack"));
        assert!(!set.has("ai_call"));
    }

    #[test]
    fn layered_capabilities_union() {
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
