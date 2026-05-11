//! Step 12 — drop terminated commit barriers from the formation's
//! active list (§12).
//!
//! `tick_commits` (step 11b) drives phase transitions and emits the
//! Prepare/Ready/Execute/Collect/Aborted events. By the time we get
//! here, any barrier in a terminal phase (Collect or Aborted) is done
//! and ready to be reclaimed. This step retires those barriers so the
//! formation doesn't retain stale state.
//!
//! We emit a final `commit_phase_changed` envelope tagged "committed"
//! when a barrier finishes cleanly so observers can distinguish a
//! Collect that drained vs one that aborted. Aborted barriers already
//! got their event from `tick_commits` (via the `record_prepare_failure`
//! or prepare-deadline transition), so we don't double-emit.

use tokio::sync::broadcast;

use crate::cooperation::formation::Formation;
use springtale_cooperation::commit::CommitPhase;
use springtale_cooperation::events::{self, CooperationEvent, CooperationEventEnvelope};

pub fn run(
    formation: &mut Formation,
    cooperation_tx: Option<&broadcast::Sender<CooperationEventEnvelope>>,
) {
    let mut committed = Vec::new();
    formation.active_commits.retain(|c| {
        if matches!(c.phase, CommitPhase::Collect) {
            committed.push(c.id);
            false
        } else if matches!(c.phase, CommitPhase::Aborted { .. }) {
            // Already announced by tick_commits / record_prepare_failure.
            false
        } else {
            true
        }
    });
    for barrier_id in committed {
        events::emit(
            cooperation_tx,
            CooperationEvent::CommitPhaseChanged {
                formation_id: formation.id,
                barrier_id,
                phase: "committed",
            },
        );
    }
}
