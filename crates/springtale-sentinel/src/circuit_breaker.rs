use std::time::{Duration, Instant};

use dashmap::DashMap;

/// Per-stage circuit breaker with three states: Closed, Open, HalfOpen.
///
/// - Closed: normal operation, counting failures
/// - Open: stage disabled after threshold failures, waiting for cooldown
/// - HalfOpen: after cooldown, allows one action to test recovery
pub struct CircuitBreaker {
    states: DashMap<String, BreakerState>,
    threshold: u32,
    cooldown: Duration,
}

#[derive(Debug, Clone)]
enum BreakerState {
    Closed { consecutive_failures: u32 },
    Open { opened_at: Instant },
    HalfOpen,
}

impl CircuitBreaker {
    pub fn new(failure_threshold: u32, cooldown: Duration) -> Self {
        Self {
            states: DashMap::new(),
            threshold: failure_threshold,
            cooldown,
        }
    }

    /// Check if the stage is allowed to execute.
    /// Returns `true` if allowed (Closed or HalfOpen), `false` if Open.
    pub fn is_allowed(&self, stage_id: &str) -> bool {
        let state =
            self.states
                .entry(stage_id.to_owned())
                .or_insert_with(|| BreakerState::Closed {
                    consecutive_failures: 0,
                });

        match state.value() {
            BreakerState::Closed { .. } => true,
            BreakerState::Open { opened_at } => {
                if opened_at.elapsed() >= self.cooldown {
                    // Cooldown expired — transition to HalfOpen
                    drop(state);
                    self.states
                        .insert(stage_id.to_owned(), BreakerState::HalfOpen);
                    true
                } else {
                    false
                }
            }
            BreakerState::HalfOpen => true,
        }
    }

    /// Report a successful execution. Resets the breaker to Closed.
    pub fn report_success(&self, stage_id: &str) {
        self.states.insert(
            stage_id.to_owned(),
            BreakerState::Closed {
                consecutive_failures: 0,
            },
        );
    }

    /// Report a failed execution. Increments failure count; opens breaker at threshold.
    pub fn report_failure(&self, stage_id: &str) {
        let mut state =
            self.states
                .entry(stage_id.to_owned())
                .or_insert_with(|| BreakerState::Closed {
                    consecutive_failures: 0,
                });

        match state.value_mut() {
            BreakerState::Closed {
                consecutive_failures,
            } => {
                *consecutive_failures += 1;
                if *consecutive_failures >= self.threshold {
                    drop(state);
                    self.states.insert(
                        stage_id.to_owned(),
                        BreakerState::Open {
                            opened_at: Instant::now(),
                        },
                    );
                    tracing::warn!(
                        stage = stage_id,
                        "circuit breaker opened after {} consecutive failures",
                        self.threshold
                    );
                }
            }
            BreakerState::HalfOpen => {
                // Failed in half-open — reopen
                drop(state);
                self.states.insert(
                    stage_id.to_owned(),
                    BreakerState::Open {
                        opened_at: Instant::now(),
                    },
                );
                tracing::warn!(
                    stage = stage_id,
                    "circuit breaker reopened after half-open failure"
                );
            }
            BreakerState::Open { .. } => {
                // Already open, no action needed
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_closed_allows() {
        let cb = CircuitBreaker::new(3, Duration::from_secs(60));
        assert!(cb.is_allowed("stage1"));
    }

    #[test]
    fn test_opens_at_threshold() {
        let cb = CircuitBreaker::new(3, Duration::from_secs(60));
        cb.report_failure("s1");
        cb.report_failure("s1");
        assert!(cb.is_allowed("s1")); // still closed (2 failures)
        cb.report_failure("s1"); // 3rd failure → opens
        assert!(!cb.is_allowed("s1")); // now open
    }

    #[test]
    fn test_success_resets() {
        let cb = CircuitBreaker::new(3, Duration::from_secs(60));
        cb.report_failure("s1");
        cb.report_failure("s1");
        cb.report_success("s1"); // reset
        cb.report_failure("s1"); // count restarts at 1
        assert!(cb.is_allowed("s1")); // still closed
    }

    #[test]
    fn test_per_stage_isolation() {
        let cb = CircuitBreaker::new(2, Duration::from_secs(60));
        cb.report_failure("a");
        cb.report_failure("a"); // opens a
        assert!(!cb.is_allowed("a"));
        assert!(cb.is_allowed("b")); // b is independent
    }

    #[test]
    fn test_half_open_success_closes() {
        let cb = CircuitBreaker::new(1, Duration::from_millis(1));
        cb.report_failure("s1"); // opens
        assert!(!cb.is_allowed("s1"));

        // Wait for cooldown
        std::thread::sleep(Duration::from_millis(5));

        assert!(cb.is_allowed("s1")); // half-open
        cb.report_success("s1"); // closes
        assert!(cb.is_allowed("s1")); // closed
    }

    #[test]
    fn test_half_open_failure_reopens() {
        let cb = CircuitBreaker::new(1, Duration::from_millis(1));
        cb.report_failure("s1"); // opens

        std::thread::sleep(Duration::from_millis(5));

        assert!(cb.is_allowed("s1")); // half-open
        cb.report_failure("s1"); // reopens
        assert!(!cb.is_allowed("s1")); // open again
    }
}
