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

impl RecoveryError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::NoPath(_) => "COOP-9001",
            Self::BudgetExceeded => "COOP-9002",
            Self::Terminal(_) => "COOP-9003",
        }
    }
}
