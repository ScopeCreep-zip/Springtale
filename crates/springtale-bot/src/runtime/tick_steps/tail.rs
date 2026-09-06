//! Per-tick cleanup that runs after every formation finishes its 25-step
//! pipeline. Three independent passes:
//!
//! - **Reclaim dead members.** L4D pattern: recoverable-dead members stay
//!   for peer revive; permanently-dead members get removed so their slots
//!   can be reused for new joiners.
//! - **Drain member bus subscriptions** (Phase 7). The protocol/state/
//!   cohesion channels otherwise fill silently because no per-member
//!   runner task exists yet to consume them. Until A2 wires per-member
//!   runner tasks, this drain prevents mpsc backlog.
//! - **Drain rally events** (Phase 8). `cascade::attempt_self_rally` and
//!   `supervise::drain` emit `RallyEvent`s into a broadcast channel; we
//!   drain it here so receivers don't lag.
//!
//! Final pass removes any formation that is no longer viable (zero
//! operational members) so the active list stays tight.

use crate::cooperation::formation::Formation;

pub fn reclaim_dead(formations: &mut [Formation]) {
    for formation in formations.iter_mut() {
        let removed = formation.remove_dead_members();
        if removed > 0 {
            tracing::info!(
                formation = %formation.id.0,
                removed,
                "reclaimed slots from dead members"
            );
        }
    }
}

pub fn drain_member_subs(formations: &mut [Formation]) {
    for formation in formations.iter_mut() {
        let counts = formation.drain_member_subs();
        let total: u32 = counts
            .iter()
            .map(|c| c.state + c.cohesion + c.protocol)
            .sum();
        if total > 0 {
            tracing::trace!(
                formation = %formation.id.0,
                total_messages = total,
                "drained member bus subscriptions"
            );
        }
    }
}

pub fn drain_rally_events(formations: &mut [Formation]) {
    for formation in formations.iter_mut() {
        for event in formation.rally.drain_events() {
            tracing::debug!(formation = %formation.id.0, ?event, "rally event");
        }
    }
}

pub fn retain_viable(formations: &mut Vec<Formation>) {
    formations.retain(|f| f.is_viable());
}
