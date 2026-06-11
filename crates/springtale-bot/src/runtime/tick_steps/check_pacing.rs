//! Step 8 — evaluate pacing transitions (`COOPERATION.md §22`, L4D Director).
//!
//! `tick.window` equals the configured tick interval (set by `CadenceBus::run`).
//! It is used here as the per-tick elapsed duration for phase transition
//! evaluation. Transitions are logged + (Phase H5) emitted onto the
//! cooperation events bus so the formation-card pacing-phase indicator
//! updates live.

use std::time::Duration;
use tokio::sync::broadcast;

use crate::cooperation::formation::Formation;
use springtale_cooperation::events::{self, CooperationEvent, CooperationEventEnvelope};
use springtale_cooperation::tick_processor::FormationTickResult;

pub fn run(
    formation: &mut Formation,
    result: &FormationTickResult,
    tick_window: Duration,
    cooperation_tx: Option<&broadcast::Sender<CooperationEventEnvelope>>,
) {
    if let Some(transition) =
        formation
            .pacing
            .evaluate_transition(result, &formation.momentum, tick_window)
    {
        tracing::info!(
            formation = %formation.id.0,
            from = %transition.from,
            to = %transition.to,
            "pacing phase transition"
        );
        events::emit(
            cooperation_tx,
            CooperationEvent::PacingPhaseChanged {
                formation_id: formation.id,
                from: transition.from,
                to: transition.to,
            },
        );
    }
}
