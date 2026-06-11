//! Failure classification — FAILURE.md protocol mapped to cooperation actions.
//!
//! Per FAILURE.md: four categories with graduated response.
//! Per AutoGen: tiered retry (attempt 1 → 2 → 3 → fallback).
//!
//! Each category maps to a cooperation layer action:
//! - GracefulDegradation → role transformation (§14)
//! - PartialFailure → rally with retry (§15)
//! - CascadingFailure → CBBA replan (L5)
//! - SilentFailure → liveness probe caught unresponsive agent

use super::liveness::Liveness;

/// Per FAILURE.md protocol: four failure categories.
///
/// The event loop's supervisor step uses this classification to decide
/// which cooperation layer handles the failure — from lightest
/// (role transform) to heaviest (escalation to L6 intervention).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureCategory {
    /// Agent alive but degraded (capability lost, performance below threshold).
    /// Response: role transformation (§14 — Siege dead→cameras).
    GracefulDegradation,
    /// Agent partially failed, isolatable with retry.
    /// Response: rally with token (§15 — Monster Hunter cart consumed).
    /// Per AutoGen: attempt with error feedback + reduced parameters.
    PartialFailure,
    /// Multiple agents failing, cascade risk detected.
    /// Response: CBBA replan (L5 — formation-wide task reallocation).
    /// Per FAILURE.md: circuit breaker fires.
    CascadingFailure,
    /// Agent stopped reporting entirely (no error, just silence).
    /// Response: mark Down, broadcast PeerMsg::AgentDown.
    /// Per FAILURE.md: "0 silent failures allowed to pass unreported."
    SilentFailure,
}

/// Classify a member's failure from observable signals.
///
/// Called per member per tick by the FormationSupervisor. Returns `None`
/// when the agent is healthy — no action needed.
pub fn classify(
    liveness: Liveness,
    consecutive_failures: usize,
    cascade_signals: u32,
) -> Option<FailureCategory> {
    // Silent failure takes priority — agent isn't even talking to us
    if liveness.is_down() {
        return Some(FailureCategory::SilentFailure);
    }

    // Cascade: multiple agents suspect + high cascade signal count
    if liveness.is_suspect() && cascade_signals >= 3 {
        return Some(FailureCategory::CascadingFailure);
    }

    // Graceful degradation: agent is alive but failing consistently
    // (5+ consecutive failures → role should transform per §14)
    if consecutive_failures >= 5 {
        return Some(FailureCategory::GracefulDegradation);
    }

    // Partial failure: agent is alive with moderate failures
    // (3+ failures → retry with rally token)
    if consecutive_failures >= 3 {
        return Some(FailureCategory::PartialFailure);
    }

    // Suspect without cascade or failure history → just observe
    None
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn healthy_agent_returns_none() {
        assert!(classify(Liveness::Alive, 0, 0).is_none());
    }

    #[test]
    fn down_agent_is_silent_failure() {
        let result = classify(
            Liveness::Down {
                since_tick: crate::tick::TickId(80),
            },
            0,
            0,
        );
        assert_eq!(result, Some(FailureCategory::SilentFailure));
    }

    #[test]
    fn suspect_with_cascade_signals_is_cascading() {
        let result = classify(Liveness::Suspect { missed_ticks: 8 }, 0, 4);
        assert_eq!(result, Some(FailureCategory::CascadingFailure));
    }

    #[test]
    fn five_failures_is_graceful_degradation() {
        let result = classify(Liveness::Alive, 5, 0);
        assert_eq!(result, Some(FailureCategory::GracefulDegradation));
    }

    #[test]
    fn three_failures_is_partial() {
        let result = classify(Liveness::Alive, 3, 0);
        assert_eq!(result, Some(FailureCategory::PartialFailure));
    }

    #[test]
    fn two_failures_still_healthy() {
        assert!(classify(Liveness::Alive, 2, 0).is_none());
    }

    #[test]
    fn down_takes_priority_over_failures() {
        let result = classify(
            Liveness::Down {
                since_tick: crate::tick::TickId(50),
            },
            10,
            5,
        );
        assert_eq!(result, Some(FailureCategory::SilentFailure));
    }
}
