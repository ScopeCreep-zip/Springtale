use crate::awareness::LocalAwareness;

use super::types::Surface;

/// Return the surfaces this agent can currently perceive.
///
/// Scaffolding policy: surfaces tagged `capability: None` are always
/// visible. Capability-tagged surfaces need the caller to pass in the
/// agent's own capability set (wired in Phase K step 5 alongside
/// CapabilityAware on `AgentContext`). The `_awareness` argument is
/// retained so the signature is stable as we tighten the filter.
pub fn visible(_awareness: &LocalAwareness, surfaces: &[Surface]) -> Vec<Surface> {
    surfaces
        .iter()
        .filter(|s| s.capability.is_none())
        .cloned()
        .collect()
}
