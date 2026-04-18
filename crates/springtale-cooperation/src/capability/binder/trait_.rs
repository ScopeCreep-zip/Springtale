use crate::capability::CapabilityDecl;
use crate::momentum::MomentumTier;

/// Project an agent's base capabilities plus the tier-unlocked
/// cooperation primitives into a flat effective set.
///
/// Implementations vary: the `DefaultBinder` appends the static tier table;
/// tests or specialised agents can substitute deterministic mocks or
/// expand-with-context logic.
pub trait Binder: Send + Sync {
    fn effective(&self, base: &[CapabilityDecl], tier: MomentumTier) -> Vec<CapabilityDecl>;
}
