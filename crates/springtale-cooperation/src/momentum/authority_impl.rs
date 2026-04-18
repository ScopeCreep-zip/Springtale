use crate::layer::{LayerAuthority, LayerId};

use super::MomentumTier;

/// Implement the layer-authority gate for the momentum FSM.
///
/// Kept as a separate file (rather than inline in `momentum/state.rs`) so
/// adding a new layer never forces a diff on momentum's own state machine —
/// the gate is a *policy* that momentum carries, not part of its transition
/// logic.
impl LayerAuthority for MomentumTier {
    fn allows(tier: MomentumTier, layer: LayerId) -> bool {
        crate::authority::allows(tier, layer)
    }
}
