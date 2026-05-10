//! Step 11 — resolve any consensus votes whose deadline has passed
//! (`COOPERATION.md §11`). Resolution is in-place on the engine; the count
//! is logged so operators can spot stalled votes.
//!
//! B7 will add the corresponding initiator path (`consensus.start_vote`) in
//! `agent/step/scan_and_claim.rs` for actions classified
//! `ApprovalPolicy::RequireConsensus`.

use crate::cooperation::formation::Formation;

pub fn run(formation: &mut Formation) {
    let resolved_votes = formation.consensus.check_deadlines();
    if !resolved_votes.is_empty() {
        tracing::info!(
            formation = %formation.id.0,
            count = resolved_votes.len(),
            "consensus votes resolved by deadline"
        );
    }
}
