use thiserror::Error;

use crate::layer::LayerId;
use crate::momentum::MomentumTier;

/// Check whether the current `tier` authorizes the given `layer`.
///
/// Free function rather than trait method so step files can call it without
/// dragging a generic parameter through. Tests can override by constructing
/// a permissive `MomentumTier` context.
pub fn allows(tier: MomentumTier, layer: LayerId) -> bool {
    super::matrix::is_allowed(tier, layer)
}

/// Gate returned when a layer-authority check fails. Kept as its own type so
/// call sites see "this cooperation layer is not unlocked at this momentum"
/// as a distinct error, not a string-comparison.
#[derive(Debug, Error, PartialEq, Eq)]
#[error("layer {layer:?} unavailable at momentum tier {tier:?}")]
pub struct Unauthorized {
    pub tier: MomentumTier,
    pub layer: LayerId,
}

/// Precondition helper: returns `Ok(())` when the tier authorises the layer,
/// `Err(Unauthorized)` otherwise. Call sites use `?` to short-circuit.
pub fn require(tier: MomentumTier, layer: LayerId) -> Result<(), Unauthorized> {
    if allows(tier, layer) {
        Ok(())
    } else {
        Err(Unauthorized { tier, layer })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn cold_blocks_l2_l3_l4_l5() {
        let t = MomentumTier::Cold;
        assert!(allows(t, LayerId::L0Ambient));
        assert!(allows(t, LayerId::L1Routine));
        assert!(!allows(t, LayerId::L2State));
        assert!(!allows(t, LayerId::L3Direct));
        assert!(!allows(t, LayerId::L4Contested));
        assert!(!allows(t, LayerId::L5Replan));
        assert!(allows(t, LayerId::L6Intervention));
        assert!(allows(t, LayerId::LInfAdmission));
    }

    #[test]
    fn fever_allows_all() {
        let t = MomentumTier::Fever;
        for l in [
            LayerId::L0Ambient,
            LayerId::L1Routine,
            LayerId::L2State,
            LayerId::L3Direct,
            LayerId::L4Contested,
            LayerId::L5Replan,
            LayerId::L6Intervention,
            LayerId::LInfAdmission,
        ] {
            assert!(allows(t, l), "Fever should allow {l:?}");
        }
    }

    #[test]
    fn hot_allows_l4_but_not_l5() {
        let t = MomentumTier::Hot;
        assert!(allows(t, LayerId::L4Contested));
        assert!(!allows(t, LayerId::L5Replan));
    }

    #[test]
    fn require_ok_on_authorized_layer() {
        assert!(require(MomentumTier::Fever, LayerId::L5Replan).is_ok());
    }

    #[test]
    fn require_err_on_unauthorized_layer() {
        let err = require(MomentumTier::Cold, LayerId::L4Contested).unwrap_err();
        assert_eq!(err.tier, MomentumTier::Cold);
        assert_eq!(err.layer, LayerId::L4Contested);
    }
}
