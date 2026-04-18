use springtale_cooperation::cadence::AgentId;
use thiserror::Error;

/// Errors from formation composition (§3.1).
///
/// Distinct from `InterventionError` (which is about runtime recovery) —
/// this is about the pre-mission admission step where the composer selects
/// agents from a candidate pool.
#[derive(Debug, Error)]
pub enum ComposeError {
    #[error("agent {0:?} not found in candidate pool")]
    AgentNotFound(AgentId),
    #[error("agent {0:?} missing required capability")]
    MissingCapability(AgentId),
    #[error("formation has no members after filtering")]
    Empty,
}
