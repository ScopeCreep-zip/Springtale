//! Diminishing Marginal Gain guard — CBBA's convergence precondition.
//!
//! Choi-Brunet-How: CBBA converges provably when every agent's scoring
//! scheme is monotone-submodular, i.e. "adding a task to a bundle does not
//! increase the bid on any task already in the bundle." `bundle::build`
//! guarantees this by construction (geometric decay per slot); this module
//! exposes a predicate so other consensus paths can assert it before
//! exchanging bids.

use super::types::Bundle;

/// Return `true` when bids are in strictly non-increasing order — the DMG
/// property for a geometric-decay bundle.
pub fn holds(bundle: &Bundle) -> bool {
    bundle.bids.windows(2).all(|w| w[0] >= w[1])
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use crate::cadence::AgentId;

    use super::*;

    #[test]
    fn empty_bundle_trivially_satisfies_dmg() {
        let b = Bundle {
            owner: AgentId::new(),
            tasks: vec![],
            bids: vec![],
            iteration: 0,
        };
        assert!(holds(&b));
    }

    #[test]
    fn non_increasing_bids_pass() {
        let b = Bundle {
            owner: AgentId::new(),
            tasks: vec![uuid::Uuid::new_v4(), uuid::Uuid::new_v4()],
            bids: vec![0.9, 0.6],
            iteration: 0,
        };
        assert!(holds(&b));
    }

    #[test]
    fn increasing_bids_fail() {
        let b = Bundle {
            owner: AgentId::new(),
            tasks: vec![uuid::Uuid::new_v4(), uuid::Uuid::new_v4()],
            bids: vec![0.3, 0.8],
            iteration: 0,
        };
        assert!(!holds(&b));
    }
}
