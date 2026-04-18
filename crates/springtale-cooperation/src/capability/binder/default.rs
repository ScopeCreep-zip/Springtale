use crate::capability::CapabilityDecl;
use crate::momentum::MomentumTier;

use super::static_table;
use super::trait_::Binder;

/// Default binder — union of base capabilities and the tier's static
/// unlocked list, de-duplicated so base capabilities that happen to share a
/// name with a coordination primitive don't appear twice.
pub struct DefaultBinder;

impl Binder for DefaultBinder {
    fn effective(&self, base: &[CapabilityDecl], tier: MomentumTier) -> Vec<CapabilityDecl> {
        let mut out: Vec<CapabilityDecl> = base.to_vec();
        for cap in static_table::unlocked_for_tier(tier) {
            let decl = CapabilityDecl::new(*cap);
            if !out.contains(&decl) {
                out.push(decl);
            }
        }
        out
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn cold_returns_base_plus_read_env_only() {
        let out =
            DefaultBinder.effective(&["github".into()], MomentumTier::Cold);
        assert!(out.contains(&"github".into()));
        assert!(out.contains(&"read_env".into()));
        assert!(!out.contains(&"consensus".into()));
    }

    #[test]
    fn fever_returns_full_unlocked_set() {
        let out = DefaultBinder.effective(&[], MomentumTier::Fever);
        for expected in ["read_env", "consensus", "ai_call", "recruit"] {
            assert!(out.contains(&CapabilityDecl::new(expected)), "missing {expected}");
        }
    }

    #[test]
    fn deduplicates_name_clashes() {
        // Contrived scenario: an agent's base list already contains a
        // coordination primitive name. The binder should not double-list it.
        let out = DefaultBinder.effective(&["read_env".into()], MomentumTier::Cold);
        let count = out.iter().filter(|c| **c == *"read_env").count();
        assert_eq!(count, 1);
    }

    #[test]
    fn higher_tier_is_superset() {
        let warming = DefaultBinder.effective(&["slack".into()], MomentumTier::Warming);
        let hot = DefaultBinder.effective(&["slack".into()], MomentumTier::Hot);
        for w in &warming {
            assert!(hot.contains(w));
        }
    }
}
