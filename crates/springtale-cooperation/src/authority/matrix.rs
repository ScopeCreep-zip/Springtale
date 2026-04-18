use crate::layer::LayerId;
use crate::momentum::MomentumTier;

/// Returns whether `tier` authorizes the given `layer`.
///
/// Matches the tier × layer table in the plan's Phase K. Cheaper primitives
/// (L0 ambient, L6 intervention, L-∞ admission) are always available; heavier
/// primitives progressively unlock as momentum rises.
pub(super) fn is_allowed(tier: MomentumTier, layer: LayerId) -> bool {
    use LayerId as L;
    use MomentumTier as M;
    match (tier, layer) {
        // Always available — ambient sense, commander override, admission.
        (_, L::L0Ambient) | (_, L::L6Intervention) | (_, L::LInfAdmission) => true,

        // L1 routine routing — all tiers can read; Cold cannot write. The
        // read/write split is enforced at the TaskRouter impl, not here.
        (_, L::L1Routine) => true,

        // L2 state dissemination + L3 direct handoff — Warming and above.
        (M::Cold, L::L2State | L::L3Direct) => false,
        (_, L::L2State | L::L3Direct) => true,

        // L4 Contract Net — Hot and above.
        (M::Cold | M::Warming, L::L4Contested) => false,
        (_, L::L4Contested) => true,

        // L5 CBBA replan — Fever only.
        (M::Fever, L::L5Replan) => true,
        (_, L::L5Replan) => false,
    }
}
