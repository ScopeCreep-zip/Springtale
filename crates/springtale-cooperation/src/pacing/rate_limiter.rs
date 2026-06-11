//! Governor GCRA rate limiter — per-phase action throughput enforcement.
//!
//! Per `COOPERATION.md §22`: `governor` 0.10 GCRA replaces a naive
//! `actions_this_minute` counter so the rate limiter handles burst and
//! jitter properly without requiring a manual reset call.
//!
//! Each phase maps to a `Quota` (actions-per-minute) via
//! `pacing::quotas::quota_for_phase`. When the phase transitions, the
//! limiter is swapped via `ArcSwapOption` — readers (e.g. concurrent
//! per-member runner tasks calling `check`) see the new limiter on the
//! next atomic load with no lock or RwLock contention. Plan §B8: "swaps
//! `Arc<RateLimiter>` via `ArcSwap` on transition."

use std::sync::Arc;

use arc_swap::ArcSwapOption;
use governor::{RateLimiter, clock::DefaultClock, state::InMemoryState};

use super::quotas::quota_for_phase;
use super::types::PacingPhase;

type Limiter = RateLimiter<governor::state::NotKeyed, InMemoryState, DefaultClock>;

/// Governor-backed rate limiter parameterised by the current `PacingPhase`.
///
/// Storage: `ArcSwapOption<Limiter>` — concurrent readers call `check()`
/// without locking; phase-transition writers call `rebuild()` which
/// performs a single atomic swap. `None` means hard-block (Disruption).
pub struct GovernorRateLimiter {
    limiter: ArcSwapOption<Limiter>,
}

impl GovernorRateLimiter {
    /// Build a limiter for the given phase. `Disruption` yields a `None`
    /// inner — all actions hard-blocked without touching the governor.
    pub fn for_phase(phase: &PacingPhase) -> Self {
        Self {
            limiter: ArcSwapOption::from(
                quota_for_phase(phase).map(|q| Arc::new(RateLimiter::direct(q))),
            ),
        }
    }

    /// Is the next action allowed under GCRA? Returns `true` and consumes
    /// a cell on success; returns `false` when rate-limited or in
    /// Disruption. Lock-free under concurrent reads — `&self` only.
    pub fn check(&self) -> bool {
        match self.limiter.load_full() {
            Some(lim) => lim.check().is_ok(),
            None => false,
        }
    }

    /// Atomically swap in a fresh limiter for a new phase. Called by
    /// `PacingManager::set_phase` on every phase transition; readers see
    /// the new quota on their next `check()` without locking.
    pub fn rebuild(&self, phase: &PacingPhase) {
        self.limiter
            .store(quota_for_phase(phase).map(|q| Arc::new(RateLimiter::direct(q))));
    }
}

impl Default for GovernorRateLimiter {
    fn default() -> Self {
        Self::for_phase(&PacingPhase::Preparation {
            started: std::time::Instant::now(),
        })
    }
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
        let lim = GovernorRateLimiter::for_phase(&PacingPhase::Preparation {
            started: Instant::now(),
        });
        assert!(lim.check());
        assert!(lim.check());
        assert!(!lim.check());
        // Transition to Active — atomic swap; budget resets with higher
        // quota. Note: rebuild now takes `&self` (ArcSwap is mutability-free).
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

    /// B8: ArcSwap swap is lock-free and visible to concurrent readers.
    /// Spawn a reader task before transition; the new quota is visible on
    /// its next check.
    #[tokio::test]
    async fn arcswap_concurrent_reader_sees_new_phase() {
        let lim = Arc::new(GovernorRateLimiter::for_phase(&PacingPhase::Recovery {
            remaining: Duration::from_secs(30),
        }));
        // Reader exhausts the 1/min Recovery budget.
        assert!(lim.check());
        assert!(!lim.check(), "Recovery 1/min exhausted");
        // Transition to Peak via &self rebuild — atomic swap.
        lim.rebuild(&PacingPhase::Peak {
            intensity: 0.9,
            fuel_rate: 2.0,
            started: Instant::now(),
        });
        let reader = lim.clone();
        let allowed = tokio::task::spawn_blocking(move || reader.check())
            .await
            .unwrap();
        assert!(allowed, "concurrent reader sees post-swap Peak budget");
    }
}
