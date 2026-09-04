use std::sync::Arc;
use std::time::Duration;

use dashmap::DashSet;
use springtale_connector::manifest::types::Capability;
use springtale_core::policy::{ApprovalPolicy, AutonomyLevel};
use springtale_core::rule::action::Action;
use springtale_store::StorageBackend;

use crate::approval::{ApprovalGate, ApprovalRequest, DefaultDenyApprovalGate};
use crate::audit::AuditTrail;
use crate::circuit_breaker::CircuitBreaker;
use crate::config::SentinelConfig;
use crate::dead_man::DeadManSwitch;
use crate::impact::{ActionHints, ActionImpact, classify_impact};
use crate::rate_limiter::RateLimiter;
use crate::throttle_tier::ThrottleTier;
use crate::toxic_pairs;
use crate::verdict::Verdict;

/// Everything the sentinel needs to evaluate one action dispatch.
///
/// Built by the runtime dispatcher (`springtale_runtime::dispatch`) from
/// the resolved action, the connector registry's manifest hints, and the
/// fire's `ExecutionContext` (policy + autonomy).
pub struct EvaluateRequest<'a> {
    /// The action about to run.
    pub action: &'a Action,
    /// Connector the action targets, or `"system"` for built-in actions.
    pub connector_name: &'a str,
    /// Momentum tier — scales the rate-limit budget.
    pub tier: ThrottleTier,
    /// The manifest's advisory hints for the named connector action.
    /// `None` when the action is not a `RunConnector`, or the connector /
    /// action is unknown — which [`classify_impact`] treats as destructive.
    pub hints: Option<ActionHints>,
    /// The connector action name for `RunConnector`, else `None`. Needed
    /// because `action_type` is the enum discriminant, which is the
    /// same string for every connector action.
    pub action_name: Option<&'a str>,
    /// The formation's `destructive_action_policy` (`AutoApprove` for
    /// global rules).
    pub policy: ApprovalPolicy,
    /// The firing member's autonomy (`ActAutonomously` for global rules).
    pub autonomy: AutonomyLevel,
}

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
    approval: Arc<dyn ApprovalGate>,
    /// `ApproveOnce` memory: (connector, action name) approved this session.
    session_approvals: DashSet<(String, String)>,
    _config: SentinelConfig,
}

impl Sentinel {
    /// Create a sentinel with the safe default approval gate
    /// ([`DefaultDenyApprovalGate`]). Destructive actions are denied
    /// unless a gate is wired via [`Sentinel::with_approval_gate`].
    pub fn new(config: SentinelConfig, store: Arc<dyn StorageBackend>) -> Self {
        Self::with_approval_gate(config, store, Arc::new(DefaultDenyApprovalGate))
    }

    /// Construct with a specific approval gate. Desktop / web wire
    /// a [`crate::approval::ChannelApprovalGate`] that prompts the
    /// user; CLI / headless callers can keep the default-deny.
    pub fn with_approval_gate(
        config: SentinelConfig,
        store: Arc<dyn StorageBackend>,
        approval: Arc<dyn ApprovalGate>,
    ) -> Self {
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
            approval,
            session_approvals: DashSet::new(),
            _config: config,
        }
    }

    /// Evaluate whether an action should proceed.
    ///
    /// Checks in order: circuit breaker, rate limiter, dead-man switch,
    /// human-approval gate. Returns the first non-Go verdict. Logs every
    /// evaluation to the audit trail.
    ///
    /// The approval gate is consulted when:
    /// - the firing member's autonomy is `ActWithApproval` (every
    ///   action, regardless of impact), or
    /// - the action classifies as `Destructive` (see [`classify_impact`],
    ///   which reads the manifest hints in `req.hints`) and the policy is
    ///   `AutoApprove` or `AlwaysRequire`, or `ApproveOnce` and this
    ///   `(connector, action name)` pair has not been approved this
    ///   session.
    ///
    /// `RequireConsensus` never prompts here: the formation vote happens
    /// before dispatch, so reaching the sentinel means the vote passed.
    ///
    /// `req.tier` scales the rate-limit budget per the cooperation
    /// framework's momentum tier (see [`ThrottleTier`]). Callers without
    /// firing context should pass [`ThrottleTier::Warming`] — the
    /// baseline budget.
    pub async fn evaluate(&self, req: EvaluateRequest<'_>) -> Verdict {
        let action_type = format!("{:?}", std::mem::discriminant(req.action));
        let impact = classify_impact(req.action, req.hints);
        let connector_name = req.connector_name;

        // 1. Circuit breaker check (per-connector) — tier-independent.
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

        // 2. Rate limiter check (per-connector, tier-scoped) — the
        //    budget scales with momentum so a Fever swarm isn't
        //    throttled to the same baseline as a Cold solo observer.
        if let Some(delay) = self.rate_limiter.check_at_tier(connector_name, req.tier) {
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

        // 4. Human-approval gate — route through the configured
        //    `ApprovalGate`. CLI / headless wire `DefaultDenyApprovalGate`
        //    so unattended runs never delete data; desktop / web wire
        //    a `ChannelApprovalGate` that prompts the survivor.
        //
        //    `ApproveOnce` is keyed on (connector, action name) — never on
        //    `action_type` alone, which is the same discriminant string
        //    for every connector action.
        let approval_key = || {
            (
                connector_name.to_owned(),
                req.action_name.unwrap_or(&action_type).to_owned(),
            )
        };
        let needs_human = match (impact, req.policy, req.autonomy) {
            (_, _, AutonomyLevel::ActWithApproval) => true,
            (ActionImpact::Destructive, ApprovalPolicy::AutoApprove, _) => true,
            (ActionImpact::Destructive, ApprovalPolicy::AlwaysRequire, _) => true,
            (ActionImpact::Destructive, ApprovalPolicy::ApproveOnce, _) => {
                !self.session_approvals.contains(&approval_key())
            }
            // RequireConsensus: the formation vote happens before dispatch;
            // reaching here means the vote passed.
            (ActionImpact::Destructive, ApprovalPolicy::RequireConsensus, _) => false,
            _ => false,
        };
        if needs_human {
            let request = ApprovalRequest {
                connector_name: connector_name.to_owned(),
                action_type: action_type.clone(),
                rationale: format!(
                    "{connector_name} is about to run {action_type} (impact {impact:?}, policy {:?}, autonomy {:?})",
                    req.policy, req.autonomy
                ),
            };
            if !self.approval.request_approval(request).await {
                let verdict =
                    Verdict::Quarantine(format!("denied by approval gate ({action_type})"));
                let _ = self
                    .audit
                    .log(connector_name, &action_type, "", &verdict, "denied")
                    .await;
                return verdict;
            }
            if matches!(req.policy, ApprovalPolicy::ApproveOnce) {
                self.session_approvals.insert(approval_key());
            }
            tracing::info!(
                connector = connector_name,
                action = %action_type,
                impact = ?impact,
                "action approved by gate"
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
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use springtale_store::{AuditFilter, SqliteBackend};

    fn test_sentinel() -> Sentinel {
        let store: Arc<dyn StorageBackend> = Arc::new(SqliteBackend::open_in_memory().unwrap());
        Sentinel::with_approval_gate(
            SentinelConfig {
                rate_limit_per_minute: 5,
                circuit_breaker_threshold: 2,
                circuit_breaker_cooldown_secs: 1,
                dead_man_threshold: 10,
                audit_retention_days: 90,
                daily_token_limit: None,
            },
            store,
            Arc::new(crate::approval::AutoAllowApprovalGate),
        )
    }

    /// A request with no hints, no action name, and the global-rule
    /// defaults (`AutoApprove` / `ActAutonomously`).
    fn req<'a>(action: &'a Action, connector: &'a str, tier: ThrottleTier) -> EvaluateRequest<'a> {
        EvaluateRequest {
            action,
            connector_name: connector,
            tier,
            hints: None,
            action_name: None,
            policy: ApprovalPolicy::AutoApprove,
            autonomy: AutonomyLevel::ActAutonomously,
        }
    }

    /// Sentinel with generous rate / breaker / dead-man budgets so the
    /// approval-gate tests exercise only the gate. Returns the store so
    /// tests can read the audit trail back.
    fn gate_sentinel(gate: Arc<dyn ApprovalGate>) -> (Sentinel, Arc<dyn StorageBackend>) {
        let store: Arc<dyn StorageBackend> = Arc::new(SqliteBackend::open_in_memory().unwrap());
        let sentinel = Sentinel::with_approval_gate(
            SentinelConfig {
                rate_limit_per_minute: 1000,
                circuit_breaker_threshold: 1000,
                circuit_breaker_cooldown_secs: 1,
                dead_man_threshold: 1000,
                audit_retention_days: 90,
                daily_token_limit: None,
            },
            store.clone(),
            gate,
        );
        (sentinel, store)
    }

    /// Gate that counts how often it is consulted and answers `allow`.
    struct CountingGate {
        calls: AtomicUsize,
        allow: bool,
    }

    #[async_trait::async_trait]
    impl ApprovalGate for CountingGate {
        async fn request_approval(&self, _request: ApprovalRequest) -> bool {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.allow
        }
    }

    fn connector_action(name: &str) -> Action {
        Action::RunConnector {
            connector: "connector-github".into(),
            action: name.into(),
            params: serde_json::Map::new(),
        }
    }

    async fn audit_results(store: &Arc<dyn StorageBackend>) -> Vec<(String, String)> {
        store
            .list_audit_entries(&AuditFilter::default())
            .await
            .unwrap()
            .into_iter()
            .map(|e| (e.verdict, e.result))
            .collect()
    }

    #[tokio::test]
    async fn test_evaluate_go() {
        let sentinel = test_sentinel();
        let action = Action::SendMessage {
            text: "hello".into(),
        };
        let verdict = sentinel
            .evaluate(req(&action, "test-connector", ThrottleTier::Warming))
            .await;
        assert_eq!(verdict, Verdict::Go);
    }

    #[tokio::test]
    async fn test_evaluate_rate_limited_at_warming_tier() {
        // Fresh sentinel with a high dead-man threshold so the 12-call
        // burst tests the rate limiter, not the dead-man switch.
        let store: Arc<dyn StorageBackend> = Arc::new(SqliteBackend::open_in_memory().unwrap());
        let sentinel = Sentinel::with_approval_gate(
            SentinelConfig {
                rate_limit_per_minute: 5,
                circuit_breaker_threshold: 1000,
                circuit_breaker_cooldown_secs: 1,
                dead_man_threshold: 1000,
                audit_retention_days: 90,
                daily_token_limit: None,
            },
            store,
            Arc::new(crate::approval::AutoAllowApprovalGate),
        );
        let action = Action::SendMessage { text: "hi".into() };

        // Warming tier budget: 12 actions per 60s.
        for i in 0..12 {
            let v = sentinel
                .evaluate(req(&action, "test", ThrottleTier::Warming))
                .await;
            assert_eq!(v, Verdict::Go, "call #{i} should be Go at Warming tier");
        }

        // 13th should throttle.
        let v = sentinel
            .evaluate(req(&action, "test", ThrottleTier::Warming))
            .await;
        assert!(matches!(v, Verdict::Throttle(_)));
    }

    #[tokio::test]
    async fn test_evaluate_cold_tier_is_more_restrictive_than_warming() {
        let sentinel = test_sentinel();
        let action = Action::SendMessage { text: "hi".into() };

        // Cold tier budget: 1 action per 30s.
        let first = sentinel
            .evaluate(req(&action, "cold-test", ThrottleTier::Cold))
            .await;
        assert_eq!(first, Verdict::Go);

        let second = sentinel
            .evaluate(req(&action, "cold-test", ThrottleTier::Cold))
            .await;
        assert!(
            matches!(second, Verdict::Throttle(_)),
            "Cold tier should throttle on 2nd call within window, got {second:?}"
        );
    }

    #[tokio::test]
    async fn test_evaluate_fever_tier_has_higher_budget_than_warming() {
        // Spawn a fresh sentinel with a higher dead_man threshold so
        // the 100-call burst doesn't trip the dead-man switch.
        let store: Arc<dyn StorageBackend> = Arc::new(SqliteBackend::open_in_memory().unwrap());
        let sentinel = Sentinel::with_approval_gate(
            SentinelConfig {
                rate_limit_per_minute: 5,
                circuit_breaker_threshold: 1000,
                circuit_breaker_cooldown_secs: 1,
                dead_man_threshold: 1000,
                audit_retention_days: 90,
                daily_token_limit: None,
            },
            store,
            Arc::new(crate::approval::AutoAllowApprovalGate),
        );
        let action = Action::SendMessage { text: "hi".into() };

        // Fever tier budget: 600 actions per 60s. 100 calls should
        // all clear without a throttle.
        for i in 0..100 {
            let v = sentinel
                .evaluate(req(&action, "fever-test", ThrottleTier::Fever))
                .await;
            assert_eq!(v, Verdict::Go, "call #{i} should be Go at Fever tier");
        }
    }

    #[tokio::test]
    async fn test_evaluate_circuit_breaker() {
        let sentinel = test_sentinel();
        let action = Action::SendMessage { text: "hi".into() };

        // Trip circuit breaker
        sentinel.report_failure("test");
        sentinel.report_failure("test");

        let v = sentinel
            .evaluate(req(&action, "test", ThrottleTier::Warming))
            .await;
        assert!(matches!(v, Verdict::Quarantine(_)));
    }

    #[tokio::test]
    async fn test_evaluate_dead_man() {
        let store: Arc<dyn StorageBackend> = Arc::new(SqliteBackend::open_in_memory().unwrap());
        let sentinel = Sentinel::with_approval_gate(
            SentinelConfig {
                rate_limit_per_minute: 1000,
                circuit_breaker_threshold: 1000,
                circuit_breaker_cooldown_secs: 1,
                dead_man_threshold: 3,
                audit_retention_days: 90,
                daily_token_limit: None,
            },
            store,
            Arc::new(crate::approval::AutoAllowApprovalGate),
        );

        let action = Action::Delay { seconds: 0 };

        // 3 actions allowed
        for _ in 0..3 {
            assert_eq!(
                sentinel
                    .evaluate(req(&action, "test", ThrottleTier::Warming))
                    .await,
                Verdict::Go
            );
        }

        // 4th triggers dead-man
        let v = sentinel
            .evaluate(req(&action, "test", ThrottleTier::Warming))
            .await;
        assert!(matches!(v, Verdict::Pause(_)));
    }

    #[tokio::test]
    async fn test_dead_man_resets_on_interaction() {
        let store: Arc<dyn StorageBackend> = Arc::new(SqliteBackend::open_in_memory().unwrap());
        let sentinel = Sentinel::with_approval_gate(
            SentinelConfig {
                rate_limit_per_minute: 1000,
                circuit_breaker_threshold: 1000,
                circuit_breaker_cooldown_secs: 1,
                dead_man_threshold: 2,
                audit_retention_days: 90,
                daily_token_limit: None,
            },
            store,
            Arc::new(crate::approval::AutoAllowApprovalGate),
        );

        let action = Action::Delay { seconds: 0 };
        sentinel
            .evaluate(req(&action, "t", ThrottleTier::Warming))
            .await;
        sentinel
            .evaluate(req(&action, "t", ThrottleTier::Warming))
            .await;

        sentinel.record_user_interaction().await;

        // After interaction, counter reset — should be Go again
        assert_eq!(
            sentinel
                .evaluate(req(&action, "t", ThrottleTier::Warming))
                .await,
            Verdict::Go
        );
    }

    #[tokio::test]
    async fn test_evaluate_always_require_destructive_auto_allow_gate_goes_with_allowed_row() {
        let (sentinel, store) = gate_sentinel(Arc::new(crate::approval::AutoAllowApprovalGate));
        let action = connector_action("delete_repository");
        let verdict = sentinel
            .evaluate(EvaluateRequest {
                policy: ApprovalPolicy::AlwaysRequire,
                action_name: Some("delete_repository"),
                ..req(&action, "connector-github", ThrottleTier::Warming)
            })
            .await;
        assert_eq!(verdict, Verdict::Go);
        let rows = audit_results(&store).await;
        assert_eq!(rows, vec![("go".to_owned(), "allowed".to_owned())]);
    }

    #[tokio::test]
    async fn test_evaluate_always_require_destructive_default_deny_gate_quarantines_with_denied_row()
     {
        let (sentinel, store) = gate_sentinel(Arc::new(DefaultDenyApprovalGate));
        let action = connector_action("delete_repository");
        let verdict = sentinel
            .evaluate(EvaluateRequest {
                policy: ApprovalPolicy::AlwaysRequire,
                action_name: Some("delete_repository"),
                ..req(&action, "connector-github", ThrottleTier::Warming)
            })
            .await;
        assert!(matches!(verdict, Verdict::Quarantine(_)), "got {verdict:?}");
        let rows = audit_results(&store).await;
        assert_eq!(rows, vec![("quarantine".to_owned(), "denied".to_owned())]);
    }

    #[tokio::test]
    async fn test_evaluate_approve_once_requests_once_then_goes_without_gate() {
        let gate = Arc::new(CountingGate {
            calls: AtomicUsize::new(0),
            allow: true,
        });
        let (sentinel, _store) = gate_sentinel(gate.clone());
        let action = connector_action("delete_repository");

        // First fire: the gate is consulted.
        let first = sentinel
            .evaluate(EvaluateRequest {
                policy: ApprovalPolicy::ApproveOnce,
                action_name: Some("delete_repository"),
                ..req(&action, "connector-github", ThrottleTier::Warming)
            })
            .await;
        assert_eq!(first, Verdict::Go);
        assert_eq!(gate.calls.load(Ordering::SeqCst), 1);

        // Second fire of the same (connector, action): remembered.
        let second = sentinel
            .evaluate(EvaluateRequest {
                policy: ApprovalPolicy::ApproveOnce,
                action_name: Some("delete_repository"),
                ..req(&action, "connector-github", ThrottleTier::Warming)
            })
            .await;
        assert_eq!(second, Verdict::Go);
        assert_eq!(gate.calls.load(Ordering::SeqCst), 1);

        // A different action on the same connector shares the enum
        // discriminant but not the memory — the gate is asked again.
        let other = connector_action("delete_branch");
        let third = sentinel
            .evaluate(EvaluateRequest {
                policy: ApprovalPolicy::ApproveOnce,
                action_name: Some("delete_branch"),
                ..req(&other, "connector-github", ThrottleTier::Warming)
            })
            .await;
        assert_eq!(third, Verdict::Go);
        assert_eq!(gate.calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn test_evaluate_act_with_approval_requests_even_for_read_only() {
        let gate = Arc::new(CountingGate {
            calls: AtomicUsize::new(0),
            allow: true,
        });
        let (sentinel, _store) = gate_sentinel(gate.clone());
        let action = Action::Delay { seconds: 0 };
        let verdict = sentinel
            .evaluate(EvaluateRequest {
                autonomy: AutonomyLevel::ActWithApproval,
                ..req(&action, "system", ThrottleTier::Warming)
            })
            .await;
        assert_eq!(verdict, Verdict::Go);
        assert_eq!(gate.calls.load(Ordering::SeqCst), 1);

        // Same stance behind a denying gate: even a read-only step stops.
        let (denying, _store) = gate_sentinel(Arc::new(DefaultDenyApprovalGate));
        let verdict = denying
            .evaluate(EvaluateRequest {
                autonomy: AutonomyLevel::ActWithApproval,
                ..req(&action, "system", ThrottleTier::Warming)
            })
            .await;
        assert!(matches!(verdict, Verdict::Quarantine(_)), "got {verdict:?}");
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
