use thiserror::Error;

use crate::cadence::AgentId;

#[derive(Debug, Error)]
pub enum RecoveryError {
    #[error("COOP-9001: no recovery path for agent {0}")]
    NoPath(AgentId),
    #[error("COOP-9002: recovery cost exceeds budget")]
    BudgetExceeded,
    #[error("COOP-9003: agent {0} at terminal failure")]
    Terminal(AgentId),
}
