//! Step 12 — expire completed or timed-out commit barriers (§12).
//!
//! Synchronized commit barriers are short-lived; once a participant has
//! reported its result (or the deadline passed), the barrier is no longer
//! useful and we drop it so the formation doesn't retain stale state.

use crate::cooperation::formation::Formation;

pub fn run(formation: &mut Formation) {
    formation
        .active_commits
        .retain(|c| !c.is_expired() && !c.is_complete());
}
