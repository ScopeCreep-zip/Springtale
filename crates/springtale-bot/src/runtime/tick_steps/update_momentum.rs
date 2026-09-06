//! Step 4 — momentum update from tick results + per-member failure tracking.
//!
//! Each tick is classified into exactly one `MomentumEvent` (see
//! [`classify`]):
//!   * `TickInterference` — interference was detected (§13).
//!   * `TickFailure` — a member acted and misaligned (alignment <= 0.5).
//!   * `TickSuccess` — at least one member acted and nothing failed.
//!   * `TickIdle` — nobody acted. Not a success, not a failure. Per the
//!     Microsoft AGT trust calibration, idle time cannot raise scores;
//!     only the decay clock keeps running.
//!
//! `tick_processor::all_succeeded` keeps its meaning (no failures, no
//! interference) for cascade and pacing; it is not a momentum signal.
//!
//! Per-member `consecutive_failures` feeds the role-transformation trigger
//! (§14) executed in `transformation::run`.

use crate::cooperation::formation::Formation;
use springtale_cooperation::momentum::{MomentumEvent, TickCounts};
use springtale_cooperation::tick_processor::FormationTickResult;
use std::collections::HashSet;

/// Classify a tick result into the single `MomentumEvent` it represents.
///
/// A report with `action_taken: None` is idle regardless of its alignment
/// (the executor reports alignment 1.0 for "nothing to do", which is not
/// a success). Only reports that actually acted can succeed or fail.
/// Success and failure carry the tick's [`TickCounts`] for the momentum
/// window.
pub fn classify(result: &FormationTickResult) -> MomentumEvent {
    let counts = count(result);
    let failed = counts.successes < counts.actions;

    if !result.interferences.is_empty() {
        MomentumEvent::TickInterference {
            count: u32::try_from(result.interferences.len()).unwrap_or(u32::MAX),
        }
    } else if failed {
        MomentumEvent::TickFailure { counts }
    } else if counts.actions > 0 {
        MomentumEvent::TickSuccess { counts }
    } else {
        MomentumEvent::TickIdle
    }
}

/// The tick's contribution to the momentum window.
///
/// `duplicates` counts acted reports whose descriptor
/// `(kind, target, payload_hash)` repeats an earlier report's in this tick.
/// `handoffs` and `handoffs_ok` are 0: `FormationTickResult` carries only
/// reports and interferences, and the `handoff::` module emits no
/// completion event the tick could read, so the handoff rate is not yet
/// measured here.
fn count(result: &FormationTickResult) -> TickCounts {
    let mut seen: HashSet<(&str, Option<&str>, u64)> = HashSet::new();
    let mut counts = TickCounts::default();
    for report in &result.reports {
        let Some(action) = report.action_taken.as_ref() else {
            continue;
        };
        counts.actions = counts.actions.saturating_add(1);
        if report.intent_alignment > 0.5 {
            counts.successes = counts.successes.saturating_add(1);
        }
        let key = (
            action.kind.as_str(),
            action.target.as_deref(),
            action.payload_hash,
        );
        if !seen.insert(key) {
            counts.duplicates = counts.duplicates.saturating_add(1);
        }
    }
    counts
}

pub fn run(formation: &mut Formation, result: &FormationTickResult) {
    // Step 4 — momentum update from actual results. A `TickSuccess` with a
    // real action also refreshes the activity clock inside `apply_event`.
    formation.momentum.apply_event(&classify(result));

    // Step 4b — per-member consecutive failures for role transformation
    // (§14). Idle and aligned reports reset the counter; a member that
    // acted and misaligned increments it.
    for report in &result.reports {
        if let Some(member) = formation.member_mut(&report.agent_id) {
            if report.action_taken.is_none() || report.intent_alignment > 0.5 {
                member.consecutive_failures = 0;
            } else {
                member.consecutive_failures += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use springtale_cooperation::cadence::{ActionDescriptor, AgentId, TickReport};
    use springtale_cooperation::tick::TickId;
    use std::time::Duration;

    fn report(action: Option<&str>, alignment: f32) -> TickReport {
        TickReport {
            agent_id: AgentId::new(),
            tick_sequence: TickId(1),
            action_taken: action.map(|kind| ActionDescriptor {
                kind: kind.to_owned(),
                target: None,
                payload_hash: 0,
            }),
            latency: Duration::from_millis(1),
            intent_alignment: alignment,
            interference_with: vec![],
        }
    }

    fn tick(reports: Vec<TickReport>) -> FormationTickResult {
        FormationTickResult {
            reports,
            interferences: vec![],
            all_succeeded: false,
        }
    }

    #[test]
    fn test_classify_no_actions_is_idle() {
        let result = tick(vec![
            report(None, 1.0),
            report(None, 1.0),
            report(None, 1.0),
        ]);
        assert!(matches!(classify(&result), MomentumEvent::TickIdle));
    }

    #[test]
    fn test_classify_empty_tick_is_idle() {
        assert!(matches!(classify(&tick(vec![])), MomentumEvent::TickIdle));
    }

    #[test]
    fn test_classify_action_aligned_is_success_and_counts_duplicates() {
        // Same kind, target and payload hash: the second report is
        // duplicate work. No handoff events reach the tick, so 0.
        let result = tick(vec![
            report(Some("work"), 1.0),
            report(Some("work"), 1.0),
            report(Some("other"), 1.0),
        ]);
        assert!(matches!(
            classify(&result),
            MomentumEvent::TickSuccess { counts }
                if counts.actions == 3
                    && counts.successes == 3
                    && counts.duplicates == 1
                    && counts.handoffs == 0
        ));
    }

    #[test]
    fn test_classify_action_misaligned_is_failure() {
        let result = tick(vec![report(Some("work"), 1.0), report(Some("work"), 0.2)]);
        assert!(matches!(
            classify(&result),
            MomentumEvent::TickFailure { counts } if counts.actions == 2 && counts.successes == 1
        ));
    }
}
