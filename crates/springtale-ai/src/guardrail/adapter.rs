//! Composable AI-adapter middleware.
//!
//! Wraps any `AiAdapter` with the Phase-7 guardrails — quota check,
//! output cap, refusal counter, wall-clock fence. The wrapper is
//! constructed by the runtime (or the factory) and stored as the
//! workspace's canonical `dyn AiAdapter` handle, so every call site
//! transparently flows through the guard.

use std::sync::Arc;

use async_trait::async_trait;

use crate::adapter::{
    AiAdapter, AiOptions, AiRequest, AiResponse, AiStream, ConnectorInfo, TokenUsage,
    ToolDefinition,
};
use crate::error::AiError;
use crate::extractor::StructuredExtractor;
use springtale_core::rule::types::Rule;

use super::output_cap::{DEFAULT_OUTPUT_CAP_BYTES, truncate_to_cap};
use super::quota::{QuotaCheck, TokenQuota};
use super::refusal::RefusalCounter;

/// Reservation handle returned by `pre_check_quota`, threaded through
/// to `commit_quota` so the backend can roll back the exact reserved
/// amount and apply the actual tokens used. Without the prior
/// reservation, `commit` would either overcount (sum reserved +
/// actual) or undercount (replace with actual but forget to release
/// the reservation).
struct Reservation {
    agent_id: String,
    reserved: u64,
}

/// `GuardrailAdapter<A>` wraps any `AiAdapter` with the workspace's
/// guardrails. All fields are optional — a default-constructed wrapper
/// is a passthrough, equivalent to the inner adapter.
///
/// Builder pattern:
///
/// ```ignore
/// let guarded = GuardrailAdapter::new(Arc::new(OllamaAdapter::new(cfg)?))
///     .with_output_cap(64 * 1024)
///     .with_refusal_counter(RefusalCounter::new())
///     .with_quota(Arc::new(InMemoryTokenQuota::new(Some(100_000))), "bot-x");
/// ```
pub struct GuardrailAdapter {
    inner: Arc<dyn AiAdapter>,
    /// Cap on `AiResponse::content.len()` returned to callers. `None`
    /// = no cap.
    output_cap: Option<usize>,
    /// Refusal-rate counter. `None` = no metric.
    refusal_counter: Option<RefusalCounter>,
    /// Per-bot quota + bot id this wrapper represents. `None` = no
    /// quota check. The id is plumbed through so a single quota
    /// backend can be shared across many `GuardrailAdapter`s, one per
    /// bot — the bot id is the key.
    quota: Option<(Arc<dyn TokenQuota>, String)>,
}

impl GuardrailAdapter {
    pub fn new(inner: Arc<dyn AiAdapter>) -> Self {
        Self {
            inner,
            output_cap: None,
            refusal_counter: None,
            quota: None,
        }
    }

    /// Truncate `AiResponse::content` past `cap_bytes`. Pass
    /// [`DEFAULT_OUTPUT_CAP_BYTES`] for the platform default.
    #[must_use]
    pub fn with_output_cap(mut self, cap_bytes: usize) -> Self {
        self.output_cap = Some(cap_bytes);
        self
    }

    /// Use the workspace default output cap (64 KiB).
    #[must_use]
    pub fn with_default_output_cap(mut self) -> Self {
        self.output_cap = Some(DEFAULT_OUTPUT_CAP_BYTES);
        self
    }

    /// Share the given `RefusalCounter` — usually a single counter
    /// from `RuntimeState` cloned into each per-bot `GuardrailAdapter`.
    #[must_use]
    pub fn with_refusal_counter(mut self, counter: RefusalCounter) -> Self {
        self.refusal_counter = Some(counter);
        self
    }

    /// Bind this wrapper to a per-bot quota. The bot id is stored once
    /// at construction; every call through this wrapper hits the same
    /// quota row.
    #[must_use]
    pub fn with_quota(mut self, quota: Arc<dyn TokenQuota>, agent_id: impl Into<String>) -> Self {
        self.quota = Some((quota, agent_id.into()));
        self
    }

    fn cap_response(&self, mut response: AiResponse) -> AiResponse {
        if let Some(cap) = self.output_cap {
            let (capped, _truncated) = truncate_to_cap(response.content, cap);
            response.content = capped;
        }
        response
    }

    /// Reserve up to `requested` tokens against the quota. Returns a
    /// reservation handle the caller threads through to `commit_quota`
    /// once the call returns — the backend uses the handle to release
    /// the reservation and book the actual count. `None` when no quota
    /// is configured. Maps `Denied` to `AiError::QuotaExceeded`.
    async fn pre_check_quota(&self, requested: u64) -> Result<Option<Reservation>, AiError> {
        let Some((quota, agent_id)) = &self.quota else {
            return Ok(None);
        };
        let outcome = quota.check_and_reserve(agent_id, requested).await?;
        match outcome {
            QuotaCheck::Allowed { .. } => Ok(Some(Reservation {
                agent_id: agent_id.clone(),
                reserved: requested,
            })),
            QuotaCheck::Denied { used, limit } => Err(AiError::QuotaExceeded {
                agent_id: agent_id.clone(),
                used,
                limit,
            }),
        }
    }

    async fn commit_quota(&self, reservation: Option<Reservation>, usage: Option<&TokenUsage>) {
        let Some(reservation) = reservation else {
            return;
        };
        let Some((quota, _)) = &self.quota else {
            return;
        };
        let actual = usage.map(|u| u64::from(u.total_tokens)).unwrap_or(0);
        if let Err(e) = quota
            .commit(&reservation.agent_id, reservation.reserved, actual)
            .await
        {
            tracing::warn!(
                error = %e,
                agent = %reservation.agent_id,
                "guardrail: quota commit failed (call already returned to caller)"
            );
        }
    }

    fn record_call(&self) {
        if let Some(counter) = &self.refusal_counter {
            counter.record_call();
        }
    }

    fn record_refusal_if_blocked(&self, err: &AiError) {
        if let (Some(counter), AiError::SanitizationBlocked { .. }) = (&self.refusal_counter, err) {
            counter.record_refusal();
        }
    }

    /// Wrap an adapter call in an explicit `tokio::time::timeout`
    /// fence on top of the transport-layer timeout. Belt-and-brace
    /// against a provider that holds the TCP connection open without
    /// returning bytes.
    async fn timed<F, T>(&self, timeout: std::time::Duration, fut: F) -> Result<T, AiError>
    where
        F: std::future::Future<Output = Result<T, AiError>>,
    {
        match tokio::time::timeout(timeout, fut).await {
            Ok(inner) => inner,
            Err(_) => Err(AiError::Timeout),
        }
    }
}

#[async_trait]
impl AiAdapter for GuardrailAdapter {
    async fn complete(
        &self,
        request: AiRequest,
        options: AiOptions,
    ) -> Result<AiResponse, AiError> {
        self.record_call();
        let reservation = self.pre_check_quota(u64::from(options.max_tokens)).await?;
        let timeout = options.timeout;
        let inner_call = self.inner.complete(request, options);
        let result = self.timed(timeout, inner_call).await;
        match result {
            Ok(response) => {
                self.commit_quota(reservation, response.usage.as_ref())
                    .await;
                Ok(self.cap_response(response))
            }
            Err(err) => {
                self.record_refusal_if_blocked(&err);
                // Roll back the reservation — actual tokens used = 0.
                self.commit_quota(reservation, None).await;
                Err(err)
            }
        }
    }

    async fn complete_with_tools(
        &self,
        request: AiRequest,
        options: AiOptions,
        tools: &[ToolDefinition],
    ) -> Result<AiResponse, AiError> {
        self.record_call();
        let reservation = self.pre_check_quota(u64::from(options.max_tokens)).await?;
        let timeout = options.timeout;
        let inner_call = self.inner.complete_with_tools(request, options, tools);
        let result = self.timed(timeout, inner_call).await;
        match result {
            Ok(response) => {
                self.commit_quota(reservation, response.usage.as_ref())
                    .await;
                Ok(self.cap_response(response))
            }
            Err(err) => {
                self.record_refusal_if_blocked(&err);
                self.commit_quota(reservation, None).await;
                Err(err)
            }
        }
    }

    async fn stream(&self, request: AiRequest, options: AiOptions) -> Result<AiStream, AiError> {
        // Streaming bypasses the output cap by design — the stream is
        // consumed incrementally, and capping mid-stream would
        // truncate the response in a way the caller couldn't tell
        // apart from a provider EOF. Quota is still checked
        // pre-flight (worst-case max_tokens) so a streaming call
        // cannot exceed the per-bot daily budget.
        self.record_call();
        let _ = self.pre_check_quota(u64::from(options.max_tokens)).await?;
        // Note: streaming does NOT commit actual tokens — the stream
        // consumer is the only place that sees finish_reason / token
        // counts, and the guardrail wrapper doesn't sit between them.
        // The reservation stays "spent" at the pessimistic max.
        let timeout = options.timeout;
        self.timed(timeout, self.inner.stream(request, options))
            .await
    }

    fn structured_extractor(&self) -> Option<&dyn StructuredExtractor> {
        self.inner.structured_extractor()
    }

    async fn parse_rule(
        &self,
        intent: &str,
        connectors: &[ConnectorInfo],
    ) -> Result<Rule, AiError> {
        self.record_call();
        let reservation = self
            .pre_check_quota(u64::from(AiOptions::default().max_tokens))
            .await?;
        let timeout = AiOptions::default().timeout;
        let inner_call = self.inner.parse_rule(intent, connectors);
        let result = self.timed(timeout, inner_call).await;
        self.commit_quota(reservation, None).await;
        result
    }

    async fn is_available(&self) -> bool {
        self.inner.is_available().await
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::adapter::ChatMessage;
    use std::sync::Arc;

    /// Minimal stub adapter — returns a fixed AiResponse, records the
    /// call count.
    struct StubAdapter {
        response_content: String,
        usage: Option<TokenUsage>,
        call_count: std::sync::atomic::AtomicU64,
    }

    impl StubAdapter {
        fn new(content: &str, tokens: Option<u32>) -> Arc<Self> {
            Arc::new(Self {
                response_content: content.to_owned(),
                usage: tokens.map(|t| TokenUsage {
                    prompt_tokens: t / 2,
                    completion_tokens: t / 2,
                    total_tokens: t,
                }),
                call_count: std::sync::atomic::AtomicU64::new(0),
            })
        }
        fn calls(&self) -> u64 {
            self.call_count.load(std::sync::atomic::Ordering::Relaxed)
        }
    }

    #[async_trait]
    impl AiAdapter for StubAdapter {
        async fn complete(
            &self,
            _request: AiRequest,
            _options: AiOptions,
        ) -> Result<AiResponse, AiError> {
            self.call_count
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Ok(AiResponse {
                content: self.response_content.clone(),
                finish_reason: Some("stop".into()),
                usage: self.usage.clone(),
                tool_calls: vec![],
            })
        }
        async fn complete_with_tools(
            &self,
            _request: AiRequest,
            _options: AiOptions,
            _tools: &[ToolDefinition],
        ) -> Result<AiResponse, AiError> {
            self.complete(AiRequest::Chat { messages: vec![] }, AiOptions::default())
                .await
        }
        async fn stream(
            &self,
            _request: AiRequest,
            _options: AiOptions,
        ) -> Result<AiStream, AiError> {
            Err(AiError::InferenceFailed(
                "stream not implemented in stub".into(),
            ))
        }
        async fn parse_rule(
            &self,
            _intent: &str,
            _connectors: &[ConnectorInfo],
        ) -> Result<Rule, AiError> {
            Err(AiError::InferenceFailed(
                "parse_rule not implemented in stub".into(),
            ))
        }
        async fn is_available(&self) -> bool {
            true
        }
    }

    fn req() -> AiRequest {
        AiRequest::Chat {
            messages: vec![ChatMessage {
                role: "user".into(),
                content: "hi".into(),
                ..Default::default()
            }],
        }
    }

    #[tokio::test]
    async fn passthrough_when_no_guardrails() {
        let stub = StubAdapter::new("hello", Some(10));
        let guarded = GuardrailAdapter::new(stub.clone());
        let resp = guarded.complete(req(), AiOptions::default()).await.unwrap();
        assert_eq!(resp.content, "hello");
        assert_eq!(stub.calls(), 1);
    }

    #[tokio::test]
    async fn output_cap_truncates_overlong_response() {
        let stub: Arc<dyn AiAdapter> = StubAdapter::new(&"x".repeat(200), Some(10));
        let guarded = GuardrailAdapter::new(stub).with_output_cap(50);
        let resp = guarded.complete(req(), AiOptions::default()).await.unwrap();
        assert!(resp.content.len() < 200);
        assert!(resp.content.contains("truncated"));
    }

    #[tokio::test]
    async fn quota_blocks_when_exceeded() {
        let stub: Arc<dyn AiAdapter> = StubAdapter::new("hello", Some(10));
        let quota = Arc::new(super::super::quota::InMemoryTokenQuota::new(Some(20)));
        let guarded = GuardrailAdapter::new(stub).with_quota(quota.clone(), "bot-1");
        // First call: max_tokens=4096 default, exceeds cap of 20 immediately.
        let err = guarded
            .complete(req(), AiOptions::default())
            .await
            .unwrap_err();
        assert!(matches!(err, AiError::QuotaExceeded { .. }));
    }

    #[tokio::test]
    async fn quota_records_actual_usage_not_reservation() {
        // The pre-reservation is the pessimistic upper bound
        // (`max_tokens`); the commit replaces it with the precise
        // actual count. Final usage MUST equal the actual tokens,
        // not the pessimistic reserve — otherwise concurrent calls
        // would each inflate the day's running total by max_tokens.
        let stub: Arc<dyn AiAdapter> = StubAdapter::new("hello", Some(10));
        let quota = Arc::new(super::super::quota::InMemoryTokenQuota::new(Some(10_000)));
        let guarded = GuardrailAdapter::new(stub).with_quota(quota.clone(), "bot-1");
        let _ = guarded.complete(req(), AiOptions::default()).await.unwrap();
        let used = quota.usage("bot-1").await.unwrap();
        assert_eq!(used, 10, "commit must replace reservation with actual");
    }

    #[tokio::test]
    async fn quota_rolls_back_reservation_on_failure() {
        // When the inner call fails, the wrapper still calls commit
        // with `actual=0` — the reservation must roll all the way
        // back so a transport failure doesn't burn a max_tokens
        // chunk of quota forever.
        struct FailingAdapter;
        #[async_trait]
        impl AiAdapter for FailingAdapter {
            async fn complete(
                &self,
                _request: AiRequest,
                _options: AiOptions,
            ) -> Result<AiResponse, AiError> {
                Err(AiError::InferenceFailed("upstream down".into()))
            }
            async fn complete_with_tools(
                &self,
                _request: AiRequest,
                _options: AiOptions,
                _tools: &[ToolDefinition],
            ) -> Result<AiResponse, AiError> {
                Err(AiError::InferenceFailed("upstream down".into()))
            }
            async fn stream(
                &self,
                _request: AiRequest,
                _options: AiOptions,
            ) -> Result<AiStream, AiError> {
                Err(AiError::InferenceFailed("upstream down".into()))
            }
            async fn parse_rule(
                &self,
                _intent: &str,
                _connectors: &[ConnectorInfo],
            ) -> Result<Rule, AiError> {
                Err(AiError::InferenceFailed("upstream down".into()))
            }
            async fn is_available(&self) -> bool {
                true
            }
        }

        let quota = Arc::new(super::super::quota::InMemoryTokenQuota::new(Some(10_000)));
        let guarded =
            GuardrailAdapter::new(Arc::new(FailingAdapter)).with_quota(quota.clone(), "bot-1");
        let _ = guarded.complete(req(), AiOptions::default()).await;
        let used = quota.usage("bot-1").await.unwrap();
        assert_eq!(used, 0, "failed call must fully roll back the reservation");
    }

    #[tokio::test]
    async fn refusal_counter_records_calls() {
        let stub: Arc<dyn AiAdapter> = StubAdapter::new("hello", Some(10));
        let counter = RefusalCounter::new();
        let guarded = GuardrailAdapter::new(stub).with_refusal_counter(counter.clone());
        for _ in 0..3 {
            let _ = guarded.complete(req(), AiOptions::default()).await;
        }
        let s = counter.snapshot();
        assert_eq!(s.total_calls, 3);
        assert_eq!(s.total_refusals, 0);
    }

    #[tokio::test]
    async fn timeout_fence_fires() {
        // Inner adapter that sleeps longer than the timeout — must
        // surface as AiError::Timeout via the wrapper's
        // tokio::time::timeout fence.
        struct SlowAdapter;
        #[async_trait]
        impl AiAdapter for SlowAdapter {
            async fn complete(
                &self,
                _request: AiRequest,
                _options: AiOptions,
            ) -> Result<AiResponse, AiError> {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                Ok(AiResponse::default())
            }
            async fn complete_with_tools(
                &self,
                _request: AiRequest,
                _options: AiOptions,
                _tools: &[ToolDefinition],
            ) -> Result<AiResponse, AiError> {
                self.complete(AiRequest::Chat { messages: vec![] }, AiOptions::default())
                    .await
            }
            async fn stream(
                &self,
                _request: AiRequest,
                _options: AiOptions,
            ) -> Result<AiStream, AiError> {
                Err(AiError::InferenceFailed("not implemented".into()))
            }
            async fn parse_rule(
                &self,
                _intent: &str,
                _connectors: &[ConnectorInfo],
            ) -> Result<Rule, AiError> {
                Err(AiError::InferenceFailed("not implemented".into()))
            }
            async fn is_available(&self) -> bool {
                true
            }
        }
        let guarded = GuardrailAdapter::new(Arc::new(SlowAdapter));
        let opts = AiOptions {
            timeout: std::time::Duration::from_millis(50),
            ..AiOptions::default()
        };
        let err = guarded.complete(req(), opts).await.unwrap_err();
        assert!(matches!(err, AiError::Timeout), "got {err:?}");
    }
}
