use crate::error::CooperationError;

/// Identifier for each architectural layer in the routing stack.
///
/// Numbered to match the plan:
/// - L0: ambient signaling (stigmergy)
/// - L1: routine routing (pull+scan + capability index)
/// - L2: state dissemination (watch + broadcast)
/// - L3: direct handoff (typed mpsc to specific agent)
/// - L4: contested allocation (Contract Net)
/// - L5: global re-plan (CBBA)
/// - L6: orchestrator intervention (push override)
/// - LInf: formation admission (one-shot at creation)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LayerId {
    L0Ambient,
    L1Routine,
    L2State,
    L3Direct,
    L4Contested,
    L5Replan,
    L6Intervention,
    LInfAdmission,
}

/// Outcome of attempting a layer's action on a given tick.
#[derive(Debug)]
pub enum LayerOutcome<T> {
    /// Layer acted and produced a result.
    Acted(T),
    /// Layer was applicable but had nothing to do.
    Skipped,
    /// Momentum tier does not authorize this layer.
    Unavailable,
    /// Layer attempted action and failed with a recoverable error.
    Failed(CooperationError),
}

pub type LayerResult<T> = Result<LayerOutcome<T>, CooperationError>;
