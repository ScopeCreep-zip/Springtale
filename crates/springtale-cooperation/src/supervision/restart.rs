//! Restart policy — Erlang OTP child_spec model for formation supervision.
//!
//! Per Erlang OTP: `MaxR` restarts in `MaxT` period. Exceeding = supervisor
//! terminates (in our case: escalates to L6 intervention).
//! Per Erlang: three strategies — one_for_one, rest_for_one, one_for_all.

/// How aggressively the formation restarts failed members.
///
/// Per Erlang OTP `child_spec`: `{MaxR, MaxT}` prevents infinite
/// restart storms. When the budget is exhausted, escalation fires.
#[derive(Debug, Clone, Copy)]
pub struct RestartPolicy {
    /// Maximum restart count within the window before escalation.
    /// Per Erlang: exceeding this triggers supervisor shutdown.
    /// Per our model: triggers L6 intervention instead of shutdown.
    pub max_restarts: u32,
    /// Window size in ticks. Restarts older than this don't count.
    pub within_ticks: u64,
    /// Which members to restart when one fails.
    pub strategy: RestartStrategy,
}

impl Default for RestartPolicy {
    fn default() -> Self {
        Self {
            max_restarts: 5,
            within_ticks: 300,
            strategy: RestartStrategy::OneForOne,
        }
    }
}

/// Per Erlang OTP: determines blast radius of a restart.
///
/// - OneForOne: only the failed member restarts (most common)
/// - RestForOne: failed member + members started after it restart
/// - OneForAll: entire formation restarts (nuclear option)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestartStrategy {
    /// Only the failed member restarts. Others unaffected.
    /// Per Erlang: most used strategy for independent workers.
    OneForOne,
    /// Failed member + all downstream dependents restart.
    /// Per Erlang: for pipeline-style dependencies where B depends on A.
    RestForOne,
    /// All members restart when any one fails.
    /// Per Erlang: for tightly coupled systems where partial state is invalid.
    OneForAll,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn default_policy_is_moderate() {
        let policy = RestartPolicy::default();
        assert_eq!(policy.max_restarts, 5);
        assert_eq!(policy.within_ticks, 300);
        assert_eq!(policy.strategy, RestartStrategy::OneForOne);
    }
}
