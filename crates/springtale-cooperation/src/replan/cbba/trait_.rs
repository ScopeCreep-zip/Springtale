use async_trait::async_trait;

use crate::action::SubTask;
use crate::agent::AgentContext;
use crate::cadence::AgentId;

use super::types::{Bundle, ConvergenceStatus};

/// Phase 1 of CBBA — greedy local bundle building against a task pool.
#[async_trait]
pub trait BundleBuilder: Send + Sync {
    async fn build(
        &self,
        agent: AgentId,
        tasks: &[SubTask],
        ctx: &AgentContext<'_>,
    ) -> Bundle;
}

/// Phase 2 of CBBA — exchange bids with neighbors until convergence.
#[async_trait]
pub trait ConsensusGossip: Send + Sync {
    async fn round(&self, bundle: &mut Bundle, neighbors: &[AgentId]) -> ConvergenceStatus;
}
