use std::time::Duration;

use async_trait::async_trait;
use uuid::Uuid;

use crate::agent::AgentContext;

use super::types::{Award, Bid, CallForProposals, CnError};

/// Formation-side initiator — announces CFPs, waits for bids, awards winners.
#[async_trait]
pub trait Initiator: Send + Sync {
    async fn announce(&self, cfp: CallForProposals) -> Uuid;
    async fn collect_bids(&self, cfp_id: Uuid, deadline: Duration) -> Vec<Bid>;
    async fn award(&self, cfp_id: Uuid, winner: Bid) -> Result<Award, CnError>;
}

/// Agent-side bidder — evaluates an incoming CFP against local state and
/// returns `Some(Bid)` if willing to commit.
#[async_trait]
pub trait Bidder: Send + Sync {
    async fn evaluate(&self, cfp: &CallForProposals, ctx: &AgentContext<'_>) -> Option<Bid>;
}
