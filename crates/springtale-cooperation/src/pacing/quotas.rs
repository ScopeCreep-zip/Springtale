//! Per-phase GCRA quota table — `PacingPhase → Quota` mapping per
//! `COOPERATION.md §22`.
//!
//! Named module per plan §B8 so the quota table has a stable, discoverable
//! home rather than living inline in `rate_limiter.rs`. Tweaking quotas
//! (e.g. raising Peak from 30 → 60 actions-per-minute) touches one file.
//!
//! L4D Director phase mapping (per `pacing/types.rs::PacingPhase` doc):
//!
//! | Phase         | L4D analogue       | Actions/min | Notes                          |
//! |---------------|--------------------|------------:|--------------------------------|
//! | Preparation   | build-up           | 2           | slow, information gathering    |
//! | Active        | sustained-peak     | 10          | normal work pace               |
//! | Peak          | sustained-peak (++)| 30          | maximum throughput, time-limited |
//! | Recovery      | peak-fade + relax  | 1           | minimal, consolidation only    |
//! | Disruption    | (interrupt)        | None (hard-block) | sentinel-detected anomaly  |

use std::num::NonZeroU32;

use governor::Quota;

use super::types::PacingPhase;

/// Map a `PacingPhase` to its per-minute action budget.
///
/// `None` means hard-block — the rate limiter rejects every check
/// without consulting GCRA. Returned by `Disruption` only.
pub fn quota_for_phase(phase: &PacingPhase) -> Option<Quota> {
    let per_min = actions_per_minute(phase)?;
    NonZeroU32::new(per_min).map(Quota::per_minute)
}

/// Raw integer action budget for a phase. Exposed for observability
/// (`PacingManager::actions_per_minute` surfaces this on the dashboard).
/// Returns `None` for `Disruption` (hard-block).
pub fn actions_per_minute(phase: &PacingPhase) -> Option<u32> {
    match phase {
        PacingPhase::Preparation { .. } => Some(2),
        PacingPhase::Active { .. } => Some(10),
        PacingPhase::Peak { .. } => Some(30),
        PacingPhase::Recovery { .. } => Some(1),
        PacingPhase::Disruption { .. } => None,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    #[test]
    fn preparation_quota_is_2_per_min() {
        let q = quota_for_phase(&PacingPhase::Preparation {
            started: Instant::now(),
        });
        assert!(q.is_some());
        assert_eq!(actions_per_minute(&PacingPhase::Preparation { started: Instant::now() }), Some(2));
    }

    #[test]
    fn disruption_quota_is_none() {
        let q = quota_for_phase(&PacingPhase::Disruption { event: "x".into() });
        assert!(q.is_none(), "Disruption hard-blocks (no GCRA budget)");
    }

    #[test]
    fn peak_quota_is_30_per_min() {
        assert_eq!(
            actions_per_minute(&PacingPhase::Peak {
                intensity: 0.9,
                fuel_rate: 2.0,
                started: Instant::now()
            }),
            Some(30)
        );
    }

    #[test]
    fn recovery_quota_is_1_per_min() {
        assert_eq!(
            actions_per_minute(&PacingPhase::Recovery {
                remaining: Duration::from_secs(30)
            }),
            Some(1)
        );
    }
}
