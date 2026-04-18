use crate::momentum::MomentumTier;

use super::types::LayerId;

/// Policy: does the current momentum tier authorize a given layer?
///
/// Implemented by `MomentumTier` (see `momentum::authority_impl`). Lives
/// behind a trait so tests can swap in a permissive matrix without touching
/// the production gates.
pub trait LayerAuthority {
    fn allows(tier: MomentumTier, layer: LayerId) -> bool;
}
