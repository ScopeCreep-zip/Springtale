use std::sync::Arc;
use std::time::Duration;

use springtale_connector::manifest::types::Capability;
use springtale_core::rule::action::Action;
use springtale_store::StorageBackend;

use crate::audit::AuditTrail;
use crate::circuit_breaker::CircuitBreaker;
use crate::config::SentinelConfig;
use crate::dead_man::DeadManSwitch;
use crate::impact::{ActionImpact, classify_impact};
use crate::rate_limiter::RateLimiter;
use crate::toxic_pairs;
use crate::verdict::Verdict;

/// Runtime behavioral monitor.
///
/// Composes rate limiting, circuit breaking, dead-man detection, and
/// audit logging. Every action dispatch should call `evaluate()` before
/// execution. The sentinel returns a `Verdict` that the dispatcher must
/// honor.
pub struct Sentinel {
    rate_limiter: RateLimiter,
    circuit_breaker: CircuitBreaker,
    dead_man: DeadManSwitch,
    audit: AuditTrail,
    _config: SentinelConfig,
}

impl Sentinel {
    /// Create a new sentinel with the given configuration and storage backend.
    pub fn new(config: SentinelConfig, store: Arc<dyn StorageBackend>) -> Self {
        let rate_limiter = RateLimiter::new(config.rate_limit_per_minute);
        let circuit_breaker = CircuitBreaker::new(
            config.circuit_breaker_threshold,
            Duration::from_secs(config.circuit_breaker_cooldown_secs),
        );
        let dead_man = DeadManSwitch::new(config.dead_man_threshold);
        let audit = AuditTrail::new(store);

        Self {
            rate_limiter,
            circuit_breaker,
            dead_man,
            audit,
            _config: config,
        }
    }

    /// Evaluate whether an action should proceed.
    ///
    /// Checks in order: circuit breaker, rate limiter, dead-man switch,
    /// destructive action gate. Returns the first non-Go verdict.
    /// Logs every evaluation to the audit trail.
    pub async fn evaluate(&self, action: &Action, connector_name: &str) -> Verdict {
        let action_type = format!("{:?}", std::mem::discriminant(action));
        let impact = classify_impact(action);

        // 1. Circuit breaker check (per-connector)
        if !self.circuit_breaker.is_allowed(connector_name) {
            let verdict = Verdict::Quarantine(format!(
                "circuit breaker open for connector: {connector_name}"
            ));
            let _ = self
                .audit
                .log(connector_name, &action_type, "", &verdict, "blocked")
                .await;
            return verdict;
        }

        // 2. Rate limiter check (per-connector)
        if let Some(delay) = self.rate_limiter.check(connector_name) {
            let verdict = Verdict::Throttle(delay);
            let _ = self
                .audit
                .log(connector_name, &action_type, "", &verdict, "throttled")
                .await;
            return verdict;
        }

        // 3. Dead-man switch check (global)
        if self.dead_man.record_action() {
            let verdict = Verdict::Pause(
                "too many actions without user interaction — dead-man switch triggered".into(),
            );
            let _ = self
                .audit
                .log(connector_name, &action_type, "", &verdict, "paused")
                .await;
            return verdict;
        }

        // 4. Destructive action gate
        if impact == ActionImpact::Destructive {
            tracing::warn!(
                connector = connector_name,
                "destructive action detected — proceeding (approval gate deferred to Phase 2b UI)"
            );
        }

        // All checks passed
        let verdict = Verdict::Go;
        let _ = self
            .audit
            .log(connector_name, &action_type, "", &verdict, "allowed")
            .await;
        verdict
    }

    /// Report successful action execution.
    pub fn report_success(&self, connector_name: &str) {
        self.circuit_breaker.report_success(connector_name);
    }

    /// Report failed action execution.
    pub fn report_failure(&self, connector_name: &str) {
        self.circuit_breaker.report_failure(connector_name);
    }

    /// Record that a user interacted (resets dead-man switch).
    pub async fn record_user_interaction(&self) {
        self.dead_man.record_user_interaction().await;
    }

    /// Check a connector's capabilities for toxic pairs.
    /// Called at install time, not at action dispatch time.
    pub fn check_toxic_pairs(
        capabilities: &[Capability],
    ) -> Result<(), crate::error::SentinelError> {
        toxic_pairs::check_toxic_pairs(capabilities)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use springtale_store::SqliteBackend;

    fn test_sentinel() -> Sentinel {
        let store: Arc<dyn StorageBackend> = Arc::new(SqliteBackend::open_in_memory().unwrap());
        Sentinel::new(
            SentinelConfig {
                rate_limit_per_minute: 5,
                circuit_breaker_threshold: 2,
                circuit_breaker_cooldown_secs: 1,
                dead_man_threshold: 10,
                audit_retention_days: 90,
            },
            store,
        )
    }

    #[tokio::test]
    async fn test_evaluate_go() {
        let sentinel = test_sentinel();
        let action = Action::SendMessage {
            text: "hello".into(),
        };
        let verdict = sentinel.evaluate(&action, "test-connector").await;
        assert_eq!(verdict, Verdict::Go);
    }

    #[tokio::test]
    async fn test_evaluate_rate_limited() {
        let sentinel = test_sentinel();
        let action = Action::SendMessage { text: "hi".into() };

        // Exhaust rate limit (5 per minute)
        for _ in 0..5 {
            let v = sentinel.evaluate(&action, "test").await;
            assert_eq!(v, Verdict::Go);
        }

        // 6th should throttle
        let v = sentinel.evaluate(&action, "test").await;
        assert!(matches!(v, Verdict::Throttle(_)));
    }

    #[tokio::test]
    async fn test_evaluate_circuit_breaker() {
        let sentinel = test_sentinel();
        let action = Action::SendMessage { text: "hi".into() };

        // Trip circuit breaker
        sentinel.report_failure("test");
        sentinel.report_failure("test");

        let v = sentinel.evaluate(&action, "test").await;
        assert!(matches!(v, Verdict::Quarantine(_)));
    }

    #[tokio::test]
    async fn test_evaluate_dead_man() {
        let store: Arc<dyn StorageBackend> = Arc::new(SqliteBackend::open_in_memory().unwrap());
        let sentinel = Sentinel::new(
            SentinelConfig {
                rate_limit_per_minute: 1000,
                circuit_breaker_threshold: 1000,
                circuit_breaker_cooldown_secs: 1,
                dead_man_threshold: 3,
                audit_retention_days: 90,
            },
            store,
        );

        let action = Action::Delay { seconds: 0 };

        // 3 actions allowed
        for _ in 0..3 {
            assert_eq!(sentinel.evaluate(&action, "test").await, Verdict::Go);
        }

        // 4th triggers dead-man
        let v = sentinel.evaluate(&action, "test").await;
        assert!(matches!(v, Verdict::Pause(_)));
    }

    #[tokio::test]
    async fn test_dead_man_resets_on_interaction() {
        let store: Arc<dyn StorageBackend> = Arc::new(SqliteBackend::open_in_memory().unwrap());
        let sentinel = Sentinel::new(
            SentinelConfig {
                rate_limit_per_minute: 1000,
                circuit_breaker_threshold: 1000,
                circuit_breaker_cooldown_secs: 1,
                dead_man_threshold: 2,
                audit_retention_days: 90,
            },
            store,
        );

        let action = Action::Delay { seconds: 0 };
        sentinel.evaluate(&action, "t").await;
        sentinel.evaluate(&action, "t").await;

        sentinel.record_user_interaction().await;

        // After interaction, counter reset — should be Go again
        assert_eq!(sentinel.evaluate(&action, "t").await, Verdict::Go);
    }

    #[test]
    fn test_toxic_pairs_safe() {
        let caps = vec![Capability::NetworkOutbound {
            host: "api.example.com".into(),
        }];
        assert!(Sentinel::check_toxic_pairs(&caps).is_ok());
    }

    #[test]
    fn test_toxic_pairs_blocked() {
        let caps = vec![
            Capability::KeychainRead {
                key: "token".into(),
            },
            Capability::NetworkOutbound {
                host: "evil.com".into(),
            },
        ];
        assert!(Sentinel::check_toxic_pairs(&caps).is_err());
    }
}
