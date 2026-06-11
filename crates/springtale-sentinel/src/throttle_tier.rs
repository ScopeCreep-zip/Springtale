//! Throttle tier — sentinel-local mirror of cooperation's
//! `MomentumTier` for momentum-aware rate limiting.
//!
//! Why a mirror, not a re-import: `springtale-sentinel` sits below
//! `springtale-cooperation` in the dependency graph (sentinel depends
//! on core + store + connector; cooperation depends on core + store).
//! Pulling cooperation into sentinel for one enum would tangle the
//! crate graph. The mirror is intentionally tiny — `springtale-runtime`
//! owns the bridge function ([`momentum_to_throttle_tier`] in
//! `crate::cooperation`) that converts between them.
//!
//! See `feedback_cooperation_over_orchestration`: cooperation
//! primitives have to thread through every action dispatch, but the
//! sentinel layer only needs the "how much budget" signal, not the
//! full cooperation context.

use std::time::Duration;

/// Per-fire momentum tier signal — controls the rate-limit budget
/// applied to the firing action.
///
/// The mapping to a per-tier (limit, window) pair lives in
/// [`Self::rate_budget`]. The pairs match the v2 plan §0.5:
///
/// | Tier    | Limit | Window |
/// |---------|-------|--------|
/// | Cold    | 1     | 30s    |
/// | Warming | 12    | 60s    |
/// | Hot     | 60    | 60s    |
/// | Fever   | 600   | 60s    |
///
/// `Warming` is the pre-Phase-0 default — when callers don't supply a
/// tier, the runtime passes `Warming` and the budget collapses to a
/// reasonable baseline (12 actions per minute per connector).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ThrottleTier {
    Cold,
    #[default]
    Warming,
    Hot,
    Fever,
}

impl ThrottleTier {
    /// Returns `(limit_per_window, window)` for this tier. The
    /// rate limiter applies these as a sliding-window cap on
    /// connector-action calls.
    pub fn rate_budget(&self) -> (u32, Duration) {
        match self {
            ThrottleTier::Cold => (1, Duration::from_secs(30)),
            ThrottleTier::Warming => (12, Duration::from_secs(60)),
            ThrottleTier::Hot => (60, Duration::from_secs(60)),
            ThrottleTier::Fever => (600, Duration::from_secs(60)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_budget_increases_with_tier() {
        let (cold_n, _) = ThrottleTier::Cold.rate_budget();
        let (warming_n, _) = ThrottleTier::Warming.rate_budget();
        let (hot_n, _) = ThrottleTier::Hot.rate_budget();
        let (fever_n, _) = ThrottleTier::Fever.rate_budget();
        assert!(cold_n < warming_n);
        assert!(warming_n < hot_n);
        assert!(hot_n < fever_n);
    }

    #[test]
    fn default_is_warming() {
        assert_eq!(ThrottleTier::default(), ThrottleTier::Warming);
    }
}
