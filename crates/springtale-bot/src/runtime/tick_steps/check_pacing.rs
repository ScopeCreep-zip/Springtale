//! Step 8 — observe stress and drive Booth's pacing loop (plan 1.5).
//!
//! `elapsed` is the true wall-clock duration since this formation's last
//! PROCESSED tick (computed in `run_tick`; `PacingManager::tick_divider`
//! decides which bus ticks are processed, so this step runs less often
//! while the formation backs off). Transitions are logged + emitted onto
//! the cooperation events bus so the formation-card indicator updates.

use std::time::Duration;
use tokio::sync::broadcast;

use crate::cooperation::dispatch_outcome::TickStress;
use crate::cooperation::formation::Formation;
use springtale_cooperation::events::{self, CooperationEvent, CooperationEventEnvelope};
use springtale_cooperation::pacing::StressSample;
use springtale_cooperation::tick_processor::FormationTickResult;

/// A report counts as harm when an action ran and its alignment sits at
/// or below this — Booth: "injured... proportional to damage taken".
const FAILURE_ALIGNMENT: f32 = 0.5;

/// Booth's increase rules for this tick: harm from the reports, sentinel
/// counts the executor accumulated on the formation, and engagement.
pub fn sample(result: &FormationTickResult, stress: TickStress, members: u32) -> StressSample {
    let acted = || result.reports.iter().filter(|r| r.action_taken.is_some());
    StressSample {
        failures: acted()
            .filter(|r| r.intent_alignment <= FAILURE_ALIGNMENT)
            .count() as u32,
        interferences: result.interferences.len() as u32,
        throttles: stress.throttles,
        denials: stress.denials,
        members,
        engaged: acted().next().is_some(),
    }
}

pub fn run(
    formation: &mut Formation,
    result: &FormationTickResult,
    elapsed: Duration,
    cooperation_tx: Option<&broadcast::Sender<CooperationEventEnvelope>>,
) {
    let stress = std::mem::take(&mut formation.tick_stress);
    let sample = sample(result, stress, formation.members.len() as u32);
    if let Some(transition) = formation.pacing.observe(&sample, elapsed) {
        tracing::info!(
            formation = %formation.id.0,
            from = %transition.from,
            to = %transition.to,
            intensity = formation.pacing.intensity,
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::cooperation::dispatch_outcome::ExecuteOutcome;
    use crate::runtime::tick_steps::build_reports::executor::test_support::successful_tick_result;
    use springtale_cooperation::cadence::AgentId;

    #[test]
    fn test_sample_counts_denials_throttles_failures_and_engagement() {
        let mut result = successful_tick_result(AgentId::new());
        let mut stress = TickStress::default();
        stress.absorb(&ExecuteOutcome {
            denied: true,
            ..ExecuteOutcome::settled(None, 0.3)
        });
        stress.absorb(&ExecuteOutcome {
            throttled: true,
            ..ExecuteOutcome::settled(None, 1.0)
        });
        assert_eq!(
            sample(&result, stress, 3),
            StressSample {
                failures: 0,
                interferences: 0,
                throttles: 1,
                denials: 1,
                members: 3,
                engaged: true,
            }
        );
        result.reports[0].intent_alignment = 0.3;
        assert_eq!(sample(&result, stress, 3).failures, 1);
        result.reports[0].action_taken = None;
        let idle = sample(&result, stress, 3);
        assert!(!idle.engaged);
        assert_eq!(idle.failures, 0);
    }
}
