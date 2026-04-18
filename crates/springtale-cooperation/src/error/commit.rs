use thiserror::Error;

use crate::cadence::AgentId;

#[derive(Debug, Error)]
pub enum CommitError {
    #[error("COOP-6001: commit barrier failed: {0}")]
    BarrierFailed(String),
    #[error("COOP-6002: prepare timed out waiting for {pending} peer(s)")]
    PrepareTimeout { pending: usize },
    #[error("COOP-6003: participant {0} dropped")]
    ParticipantDropped(String),
    /// Agent signalled on a barrier they aren't part of. Distinct from a
    /// generic `FormationError::AgentNotFound` because the lookup scope is
    /// this barrier, not the formation.
    #[error("COOP-6004: agent {0:?} not a participant in this barrier")]
    AgentNotInBarrier(AgentId),
    #[error("COOP-6005: execution phase failed for {0:?}: {1}")]
    ExecutionFailed(AgentId, String),
}
