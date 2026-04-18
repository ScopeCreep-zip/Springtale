//! Governor GCRA rate limiter — per-phase action throughput enforcement.
//!
//! Per COOPERATION.md §22: `governor` 0.8 GCRA replaces the naive
//! `actions_this_minute` counter so the rate limiter handles burst and
//! jitter properly without requiring a manual reset call.
//!
//! Each phase maps to a `Quota` (actions-per-minute). When the phase
//! transitions, the limiter is rebuilt with the new quota — `RateLimiter`
//! creation is ~200 ns, negligible per transition.

use std::num::NonZeroU32;

use governor::{clock::DefaultClock, state::InMemoryState, Quota, RateLimiter};

use super::types::PacingPhase;

type Limiter = RateLimiter<governor::state::NotKeyed, InMemoryState, DefaultClock>;

/// Governor-backed rate limiter parameterised by the current `PacingPhase`.
///
/// Callers call `check()` before submitting an action; the GCRA algorithm
/// decides whether the action fits within the current phase's throughput
/// budget.
pub struct GovernorRateLimiter {
    limiter: Option<Limiter>,
}

impl GovernorRateLimiter {
    /// Build a limiter for the given phase. `Disruption` yields `None` —
    /// all actions hard-blocked without touching the governor.
    pub fn for_phase(phase: &PacingPhase) -> Self {
        Self {
            limiter: quota_for_phase(phase).map(RateLimiter::direct),
        }
    }

    /// Is the next action allowed under GCRA? Returns `true` and consumes a
    /// cell on success; returns `false` when rate-limited or in Disruption.
    pub fn check(&self) -> bool {
        match &self.limiter {
            Some(lim) => lim.check().is_ok(),
            None => false,
        }
    }

    /// Rebuild for a new phase. Called by `PacingManager` on every phase
    /// transition.
    pub fn rebuild(&mut self, phase: &PacingPhase) {
        self.limiter = quota_for_phase(phase).map(RateLimiter::direct);
    }
}

impl Default for GovernorRateLimiter {
    fn default() -> Self {
        Self::for_phase(&PacingPhase::Preparation {
            started: std::time::Instant::now(),
        })
    }
}

/// Phase → GCRA quota table (spec §22):
/// - Preparation: 2/min (slow, information gathering)
/// - Active: 10/min (normal work pace)
/// - Peak: 30/min (maximum throughput)
/// - Recovery: 1/min (minimal, consolidation only)
/// - Disruption: None (hard-block, no governor)
fn quota_for_phase(phase: &PacingPhase) -> Option<Quota> {
    let per_min = match phase {
        PacingPhase::Preparation { .. } => 2,
        PacingPhase::Active { .. } => 10,
        PacingPhase::Peak { .. } => 30,
        PacingPhase::Recovery { .. } => 1,
        PacingPhase::Disruption { .. } => return None,
    };
    NonZeroU32::new(per_min).map(Quota::per_minute)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    #[test]
    fn preparation_allows_two_then_blocks() {
        let lim = GovernorRateLimiter::for_phase(&PacingPhase::Preparation {
            started: Instant::now(),
        });
        assert!(lim.check(), "1st action in Preparation");
        assert!(lim.check(), "2nd action in Preparation");
        assert!(!lim.check(), "3rd action should be blocked at 2/min");
    }

    #[test]
    fn peak_allows_thirty_burst() {
        let lim = GovernorRateLimiter::for_phase(&PacingPhase::Peak {
            intensity: 0.9,
            fuel_rate: 2.0,
            started: Instant::now(),
        });
        for i in 0..30 {
            assert!(lim.check(), "action {i} should pass at Peak");
        }
        assert!(!lim.check(), "31st should be blocked at 30/min");
    }

    #[test]
    fn disruption_blocks_all() {
        let lim = GovernorRateLimiter::for_phase(&PacingPhase::Disruption {
            event: "connector_timeout".into(),
        });
        assert!(!lim.check());
    }

    #[test]
    fn rebuild_resets_budget() {
        let mut lim = GovernorRateLimiter::for_phase(&PacingPhase::Preparation {
            started: Instant::now(),
        });
        assert!(lim.check());
        assert!(lim.check());
        assert!(!lim.check());
        // Transition to Active — budget resets with higher quota.
        lim.rebuild(&PacingPhase::Active {
            intensity: 0.5,
            started: Instant::now(),
        });
        assert!(lim.check(), "first action after rebuild to Active");
    }

    #[test]
    fn recovery_allows_one() {
        let lim = GovernorRateLimiter::for_phase(&PacingPhase::Recovery {
            remaining: Duration::from_secs(30),
        });
        assert!(lim.check(), "1st action in Recovery");
        assert!(!lim.check(), "2nd action should block at 1/min");
    }
}
