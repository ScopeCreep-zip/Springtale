//! Step 4e — per-member fuel consumption.
//!
//! Each member with an active task burns one unit of fuel per tick. This is
//! the per-agent budget; the formation-level budget is a separate ledger
//! enforced by the orchestrator's `FuelBudget` (`crates/springtale-bot/src/
//! orchestrator/fuel.rs`).

use crate::cooperation::formation::Formation;

pub fn run(formation: &mut Formation) {
    for member in &mut formation.members {
        if member.active_task.is_some() {
            member.fuel_remaining.consume(1).ok();
        }
    }
}
