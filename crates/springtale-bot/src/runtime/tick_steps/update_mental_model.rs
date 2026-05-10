//! Step 13 — update the formation's `SharedMentalModel` from this tick's
//! reports + interferences (`COOPERATION.md §21`).
//!
//! Per-tick learning so accumulated cooperation patterns / vocabulary /
//! conventions are kept fresh for the formation's full lifetime. The model
//! is persisted on dissolve (`crates/springtale-bot/src/cooperation/
//! lifecycle.rs::persist_mental_model`).
//!
//! G2 will additionally fold this back into a global mental_model_state row
//! so subsequent formations share what their predecessors learned.

use crate::cooperation::formation::Formation;
use springtale_cooperation::mental_model::learning;
use springtale_cooperation::tick_processor::FormationTickResult;

pub fn run(formation: &mut Formation, result: &FormationTickResult) {
    learning::update_model(
        &mut formation.mental_model,
        &result.reports,
        &result.interferences,
        result.all_succeeded,
    );
}
