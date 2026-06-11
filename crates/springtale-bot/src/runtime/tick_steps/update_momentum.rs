//! Step 4 — momentum update from tick results + per-member failure tracking.
//!
//! Three sources of momentum signal:
//!   * `record_success` when every report aligned and no interference.
//!   * `record_interference` once per detected interference event (§13).
//!   * `record_failure` when no interference but at least one misalignment.
//!
//! Per-member `consecutive_failures` feeds the role-transformation trigger
//! (§14) executed in `transformation::run`.

use crate::cooperation::formation::Formation;
use springtale_cooperation::tick_processor::FormationTickResult;

pub fn run(formation: &mut Formation, result: &FormationTickResult) {
    // Step 4 — momentum update from actual results.
    if result.all_succeeded {
        formation.momentum.record_success();
    } else if !result.interferences.is_empty() {
        for _ in &result.interferences {
            formation.momentum.record_interference();
        }
    } else {
        formation.momentum.record_failure();
    }

    // Step 4a — record real activity (only when agents actually acted).
    // Decay tracks real work, not tick heartbeats; idle ticks should not
    // refresh the activity timer.
    let had_real_actions = result.reports.iter().any(|r| r.action_taken.is_some());
    if had_real_actions {
        formation.momentum.record_activity();
    }

    // Step 4b — per-member consecutive failures for role transformation
    // (§14). Aligned reports reset the counter; misaligned ones increment.
    for report in &result.reports {
        if let Some(member) = formation.member_mut(&report.agent_id) {
            if report.intent_alignment > 0.5 {
                member.consecutive_failures = 0;
            } else {
                member.consecutive_failures += 1;
            }
        }
    }
}
