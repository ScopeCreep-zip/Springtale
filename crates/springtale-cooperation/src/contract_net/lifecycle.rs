use std::time::Instant;

use crate::cadence::AgentId;
use uuid::Uuid;

use super::types::Bid;

/// Lifecycle states of a single CFP round. Strictly sequential — each
/// transition is driven by one function in this module, which makes the
/// protocol's state machine auditable from one file.
#[derive(Debug, Clone)]
pub enum CfpState {
    Announced {
        cfp_id: Uuid,
        started_at: Instant,
    },
    Collecting {
        cfp_id: Uuid,
        bids: Vec<Bid>,
    },
    Awarded {
        cfp_id: Uuid,
        winner: AgentId,
        utility: f32,
    },
    Expired {
        cfp_id: Uuid,
    },
}
