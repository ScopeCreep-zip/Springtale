//! Step 4 — momentum update from tick results + per-member failure tracking.
//!
//! Each tick is classified into exactly one `MomentumEvent` (see
//! [`classify`]):
//!   * `TickInterference` — interference was detected (§13).
//!   * `TickFailure` — a member finished work that failed or misaligned.
//!   * `TickSuccess` — at least one member finished work and nothing failed.
//!   * `TickIdle` — nobody finished work. Not a success, not a failure. Per the
//!     Microsoft AGT trust calibration, idle time cannot raise scores;
//!     only the decay clock keeps running.
//!
//! `tick_processor::all_succeeded` keeps its meaning (no failures, no
//! interference) for cascade and pacing; it is not a momentum signal.
//!
//! Per-member `consecutive_failures` feeds the role-transformation trigger
//! (§14) executed in `transformation::run`.

use crate::cooperation::formation::Formation;
use springtale_cooperation::action_state::ActionState;
use springtale_cooperation::cadence::TickReport;
use springtale_cooperation::momentum::{MomentumEvent, TickCounts};
use springtale_cooperation::tick_processor::FormationTickResult;
use springtale_cooperation::utterance::{UtteranceKind, utter};

/// Idle ticks before a member says `Listening` (plan §1.15 E).
pub const LISTENING_AFTER_TICKS: u32 = 5;
use std::collections::HashSet;

/// Whether this report is work the beat actually finished.
///
/// The report's [`ActionState`] is the source, not `action_taken` and not
/// `intent_alignment`. Several non-work paths surface a descriptor with a
/// high alignment — a dispatch carried past its beat reports `Requested`
/// at 0.8, a continued active task reports 1.0 while the task is merely
/// claimed, a sacrifice yield reports 0.9, and the observe, suggest and
/// no-task paths report the step's surface reaction at 1.0. None of those
/// finished anything, so none of them may move momentum: a hung connector
/// call or an observe-autonomy member must not walk a formation to Fever.
fn completed_work(report: &TickReport) -> bool {
    report.state.is_terminal()
}

/// Whether the finished work counted as a success: the action reached
/// `Success` *and* aligned with the formation's intent.
fn succeeded(report: &TickReport) -> bool {
    matches!(report.state, ActionState::Success) && report.intent_alignment > 0.5
}

/// Classify a tick result into the single `MomentumEvent` it represents.
///
/// A report that did not reach a terminal action state is idle regardless
/// of its alignment — waiting and claimed-only are not success. Only
/// reports that finished work can succeed or fail. Success and failure
/// carry the tick's [`TickCounts`] for the momentum window.
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
        if !completed_work(report) {
            continue;
        }
        counts.actions = counts.actions.saturating_add(1);
        if succeeded(report) {
            counts.successes = counts.successes.saturating_add(1);
        }
        let Some(action) = report.action_taken.as_ref() else {
            continue;
        };
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

pub fn run(
    formation: &mut Formation,
    result: &FormationTickResult,
    cooperation_tx: Option<
        &tokio::sync::broadcast::Sender<springtale_cooperation::CooperationEventEnvelope>,
    >,
) {
    // Step 4 — momentum update from actual results. A `TickSuccess` with a
    // real action also refreshes the activity clock inside `apply_event`.
    formation.momentum.apply_event(&classify(result));

    // Step 4b — per-member consecutive failures for role transformation
    // (§14). Idle reports and finished-and-aligned work reset the counter;
    // only a member whose work finished badly increments it. A member
    // still waiting on a dispatch is neither.
    for report in &result.reports {
        let mut now_listening = false;
        if let Some(member) = formation.member_mut(&report.agent_id) {
            if report.action_taken.is_none() {
                member.consecutive_idle_ticks = member.consecutive_idle_ticks.saturating_add(1);
                now_listening = member.consecutive_idle_ticks == LISTENING_AFTER_TICKS;
            } else {
                member.consecutive_idle_ticks = 0;
            }
            if !completed_work(report) || succeeded(report) {
                member.consecutive_failures = 0;
            } else {
                member.consecutive_failures += 1;
            }
        }
        if now_listening {
            utter(
                &mut formation.utter_ctx(cooperation_tx),
                Some(report.agent_id),
                UtteranceKind::Listening,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cooperation::dispatch_outcome::REQUESTED_ALIGNMENT;
    use springtale_cooperation::cadence::{ActionDescriptor, AgentId, TickReport};
    use springtale_cooperation::momentum::{MomentumState, MomentumTier};
    use springtale_cooperation::tick::TickId;
    use std::time::Duration;

    fn stated(action: Option<&str>, alignment: f32, state: ActionState) -> TickReport {
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
            surface_reaction: None,
            state,
        }
    }

    /// A report for work that finished this beat (or for an idle member).
    fn report(action: Option<&str>, alignment: f32) -> TickReport {
        let state = match action {
            Some(_) => ActionState::Success,
            None => ActionState::Init,
        };
        stated(action, alignment, state)
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

    /// Fix 1 — a hung dispatch does not promote.
    ///
    /// A connector call carried past its beat reports `Requested` with a
    /// descriptor and alignment 0.8. Under the old alignment-only rule
    /// that was a success every beat, so a formation whose members were
    /// all stuck walked itself to Fever. It is idle, and idle never
    /// promotes.
    #[test]
    fn test_hung_dispatch_is_idle_and_never_promotes() {
        let hung = || stated(Some("work"), REQUESTED_ALIGNMENT, ActionState::Requested);
        let result = tick(vec![hung(), hung()]);
        assert!(matches!(classify(&result), MomentumEvent::TickIdle));

        let mut momentum = MomentumState::default();
        for _ in 0..50 {
            momentum.apply_event(&classify(&result));
        }
        assert_eq!(momentum.tier, MomentumTier::Cold);
        assert_eq!(momentum.consecutive_successes, 0);
    }

    /// A claim, an observe-autonomy surface reaction and a sacrifice
    /// yield all report a descriptor at high alignment without finishing
    /// anything. None of them is a success.
    #[test]
    fn test_claimed_and_observed_reports_are_idle() {
        let result = tick(vec![
            stated(Some("claimed"), 1.0, ActionState::Init),
            stated(Some("sacrifice_yield"), 0.9, ActionState::Init),
            stated(Some("cancelled"), 1.0, ActionState::Cancelled),
        ]);
        assert!(matches!(classify(&result), MomentumEvent::TickIdle));
    }
}
