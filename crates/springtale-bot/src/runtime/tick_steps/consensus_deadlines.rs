//! Step 11 — resolve any consensus votes whose deadline has passed
//! (`COOPERATION.md §11`). Resolution is in-place on the engine; the count
//! is logged so operators can spot stalled votes.
//!
//! B7 will add the corresponding initiator path (`consensus.start_vote`) in
//! `agent/step/scan_and_claim.rs` for actions classified
//! `ApprovalPolicy::RequireConsensus`.

use tokio::sync::broadcast;

use crate::cooperation::formation::Formation;
use springtale_cooperation::events::{
    self, CooperationEvent, CooperationEventEnvelope, VoteOutcome,
};

pub fn run(
    formation: &mut Formation,
    cooperation_tx: Option<&broadcast::Sender<CooperationEventEnvelope>>,
) {
    let resolved_votes = formation.consensus.check_deadlines();
    if !resolved_votes.is_empty() {
        tracing::info!(
            formation = %formation.id.0,
            count = resolved_votes.len(),
            "consensus votes resolved by deadline"
        );
        for (vote_id, _resolution) in resolved_votes {
            // Deadline-resolved votes are timeouts (no quorum reached).
            // The Approved/Denied paths fire when an agent records the
            // decisive vote — wired separately as the consensus engine
            // gains a callback hook.
            events::emit(
                cooperation_tx,
                CooperationEvent::ConsensusVoteResolved {
                    formation_id: formation.id,
                    vote_id,
                    outcome: VoteOutcome::Timeout,
                },
            );
        }
    }
}
