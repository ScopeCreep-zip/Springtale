use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::action::SubTask;
use crate::cadence::AgentId;
use crate::capability::CapabilityDecl;

/// FIPA-CNP "call-for-proposals" message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallForProposals {
    pub id: Uuid,
    pub initiator: AgentId,
    pub task: SubTask,
    pub deadline: Duration,
    pub required_capability: Option<CapabilityDecl>,
    pub scoring_hint: Option<String>,
}

/// A participant's bid. Utility score is 0.0..=1.0 per `utility/` module.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bid {
    pub cfp_id: Uuid,
    pub bidder: AgentId,
    pub utility: f32,
    pub estimated_completion: Duration,
    pub rationale: String,
}

/// Award message — sent to winner + rejection to all others.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Award {
    pub cfp_id: Uuid,
    pub winner: AgentId,
    pub utility: f32,
}

#[derive(Debug, Error)]
pub enum CnError {
    #[error("no bids received for CFP {0}")]
    NoBids(Uuid),
    #[error("CFP {0} deadline elapsed before award")]
    DeadlineExpired(Uuid),
    #[error("CFP {0} not found")]
    NotFound(Uuid),
    #[error("CFP already awarded: {0}")]
    AlreadyAwarded(Uuid),
}
