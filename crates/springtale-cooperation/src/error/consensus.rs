use thiserror::Error;

use crate::cadence::AgentId;

#[derive(Debug, Error)]
pub enum ConsensusError {
    #[error("COOP-5001: no override tokens remaining for agent {0:?}")]
    NoOverrideTokens(AgentId),
    #[error("COOP-5002: consensus deadline expired")]
    Timeout,
    #[error("COOP-5003: vote not found: {0}")]
    VoteNotFound(uuid::Uuid),
}
