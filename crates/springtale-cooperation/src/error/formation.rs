use thiserror::Error;

use crate::cadence::AgentId;
use crate::capability::CapabilityDecl;

#[derive(Debug, Error)]
pub enum FormationError {
    #[error("COOP-2001: agent not found: {0}")]
    AgentNotFound(AgentId),
    #[error("COOP-2002: formation is empty")]
    Empty,
    #[error("COOP-2003: formation not viable: {0}")]
    NotViable(String),
    #[error("COOP-2004: missing required capability: {0}")]
    MissingCapability(CapabilityDecl),
    #[error("COOP-2005: formation context not initialized")]
    ContextUninit,
}
