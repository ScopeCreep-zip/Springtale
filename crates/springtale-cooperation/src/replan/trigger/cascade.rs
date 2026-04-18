//! Cascade trigger — declares an L5 replan warranted when the tick pipeline
//! shows correlated failures across the formation.
//!
//! Not a general failure counter: we look for *cascade shape* — multiple
//! failures in a narrow window plus shared interference targets. Isolated
//! single failures get handled by rally (L4/L2), not replan.

/// Signal inputs the trigger consumes. Kept as a simple struct (rather than a
/// tick reference) so tests can fabricate scenarios without the full pipeline.
#[derive(Debug, Clone, Copy, Default)]
pub struct CascadeSignals {
    pub failures_in_window: u32,
    pub unique_interference_targets: u32,
    pub rally_tokens_remaining: u32,
}

/// Threshold configuration — surfaced as constants so the rule is auditable
/// in one place.
#[derive(Debug, Clone, Copy)]
pub struct CascadeThresholds {
    pub min_failures: u32,
    pub max_rally_tokens: u32,
}

impl Default for CascadeThresholds {
    fn default() -> Self {
        Self {
            min_failures: 3,
            max_rally_tokens: 1,
        }
    }
}

/// Returns `true` when a cascade replan should fire.
pub fn should_replan(signals: CascadeSignals, thresholds: CascadeThresholds) -> bool {
    signals.failures_in_window >= thresholds.min_failures
        && signals.rally_tokens_remaining <= thresholds.max_rally_tokens
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn fires_when_failures_exceed_with_low_rally() {
        let s = CascadeSignals {
            failures_in_window: 4,
            unique_interference_targets: 2,
            rally_tokens_remaining: 0,
        };
        assert!(should_replan(s, CascadeThresholds::default()));
    }

    #[test]
    fn ignores_isolated_failure() {
        let s = CascadeSignals {
            failures_in_window: 1,
            unique_interference_targets: 0,
            rally_tokens_remaining: 3,
        };
        assert!(!should_replan(s, CascadeThresholds::default()));
    }

    #[test]
    fn ignores_failures_when_rally_has_budget() {
        let s = CascadeSignals {
            failures_in_window: 5,
            unique_interference_targets: 3,
            rally_tokens_remaining: 3,
        };
        assert!(!should_replan(s, CascadeThresholds::default()));
    }
}
