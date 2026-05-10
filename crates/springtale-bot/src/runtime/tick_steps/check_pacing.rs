//! Step 8 — evaluate pacing transitions (`COOPERATION.md §22`, L4D Director).
//!
//! `tick.window` equals the configured tick interval (set by `CadenceBus::run`).
//! It is used here as the per-tick elapsed duration for phase transition
//! evaluation. Transitions are logged; B8 will additionally swap the
//! per-formation rate limiter on transition.

use std::time::Duration;

use crate::cooperation::formation::Formation;
use springtale_cooperation::tick_processor::FormationTickResult;

pub fn run(formation: &mut Formation, result: &FormationTickResult, tick_window: Duration) {
    if let Some(transition) = formation
        .pacing
        .evaluate_transition(result, &formation.momentum, tick_window)
    {
        tracing::info!(
            formation = %formation.id.0,
            from = %transition.from,
            to = %transition.to,
            "pacing phase transition"
        );
    }
}
