//! `ExecutionRecorder` trait + production / no-op impls.
//!
//! The trait sits between the dispatcher and the store. The
//! dispatcher calls `begin` / `record_step` / `finish` around each
//! chain fire; the trait implementation owns the privacy posture
//! (sizes-only by default, opt-in content via blob refs in a
//! later phase).
//!
//! Three impls today:
//!
//! - [`StoreRecorder`] — production. Writes through the wired
//!   [`StorageBackend`] using the executions / execution_steps
//!   tables (Phase B.5). Privacy invariant enforced here: no
//!   content fields, only sizes / kinds / enum-tagged error.
//!
//! - [`NoopRecorder`] — test-only. Drops every call on the floor
//!   so dispatch tests that don't care about persistence stay
//!   short.
//!
//! - [`MemoryRecorder`] — test fixture that accumulates the
//!   rows in-process so tests can assert on what the dispatcher
//!   wrote without standing up a real SQLite. Lives in
//!   `#[cfg(test)]` only.

use std::sync::Arc;

use async_trait::async_trait;

use springtale_core::rule::StepOutput;
use springtale_cooperation::execution::{ExecutionContext, ExecutionMode as CoopExecutionMode};
use springtale_cooperation::momentum::MomentumTier;
use springtale_store::backend::StorageBackend;
use springtale_store::schema::executions::{
    ExecutionMode as StoreExecutionMode, ExecutionRow, ExecutionStatus, ExecutionStepRow,
    MomentumTag, StepStatus,
};

use crate::error::OperationError;

/// Default retention for an executions row, in milliseconds.
/// Stricter than Apify (30 days) and n8n (no default cap) per the
/// privacy invariant — the user can extend this per-bot in
/// Phase C, but the default must be safe.
pub const DEFAULT_RETENTION_MS: i64 = 14 * 24 * 3600 * 1000;

/// Records lifecycle events for a chain fire. Implementations sit
/// behind the bridge so the dispatcher can call them without
/// hardcoding any specific backend.
#[async_trait]
pub trait ExecutionRecorder: Send + Sync + 'static {
    /// Insert the per-fire row. Called before the first step runs.
    /// `recipe_id` is best-effort — the dispatcher passes the rule's
    /// associated recipe id when it knows it (Phase A+ rule rows
    /// carry one).
    async fn begin(
        &self,
        execution: &ExecutionContext,
        trigger_summary: &str,
        recipe_id: Option<&str>,
    ) -> Result<(), OperationError>;

    /// Append one step row. Called after each successful
    /// [`StepOutput`] is built by the dispatcher's run_step. Sizes
    /// derived from `step.output` JSON length; never the content
    /// itself.
    async fn record_step(
        &self,
        execution_id: &str,
        step: &StepOutput,
    ) -> Result<(), OperationError>;

    /// Finalize the per-fire row. Always called from the
    /// dispatcher's top-level entry once the chain returns —
    /// success, failure, suppression, or abort.
    async fn finish(
        &self,
        execution_id: &str,
        status: ExecutionStatus,
        error_kind: Option<&str>,
    ) -> Result<(), OperationError>;
}

/// Production recorder — writes to the store via the
/// `StorageBackend` trait's executions methods.
pub struct StoreRecorder {
    store: Arc<dyn StorageBackend>,
    retention_ms: i64,
}

impl StoreRecorder {
    pub fn new(store: Arc<dyn StorageBackend>) -> Self {
        Self {
            store,
            retention_ms: DEFAULT_RETENTION_MS,
        }
    }

    /// Build a recorder with a custom retention window. Used by
    /// Phase C's per-bot config to widen or shorten the default.
    pub fn with_retention_ms(mut self, retention_ms: i64) -> Self {
        self.retention_ms = retention_ms;
        self
    }

    fn now_ms() -> i64 {
        chrono::Utc::now().timestamp_millis()
    }
}

#[async_trait]
impl ExecutionRecorder for StoreRecorder {
    async fn begin(
        &self,
        execution: &ExecutionContext,
        trigger_summary: &str,
        recipe_id: Option<&str>,
    ) -> Result<(), OperationError> {
        let started_at = Self::now_ms();
        let row = ExecutionRow {
            id: execution.execution_id.to_string(),
            bot_id: execution.agent_id.as_ref().map(|a| a.0.to_string()),
            formation_id: execution.formation_id.as_ref().map(|f| f.0.to_string()),
            rule_id: Some(execution.rule_id.0.to_string()),
            recipe_id: recipe_id.map(str::to_owned),
            started_at,
            finished_at: None,
            mode: map_mode(execution.mode),
            status: ExecutionStatus::Running,
            momentum: Some(map_momentum(execution.momentum)),
            trigger_summary: Some(trigger_summary.to_owned()),
            error_kind: None,
            duration_ms: None,
            retention_until: started_at.saturating_add(self.retention_ms),
            retry_of: None,
        };
        self.store
            .record_execution_start(row)
            .await
            .map_err(OperationError::from)?;
        Ok(())
    }

    async fn record_step(
        &self,
        execution_id: &str,
        step: &StepOutput,
    ) -> Result<(), OperationError> {
        let now = Self::now_ms();
        let started = now.saturating_sub(step.duration_ms as i64);
        // Sizes only — the privacy invariant. We measure the
        // serialized output length so the executions panel can
        // render "step produced 1.2KB" without ever persisting
        // the bytes.
        let output_bytes = serde_json::to_vec(&step.output)
            .map(|v| v.len() as i64)
            .unwrap_or(0);
        let output_kind = classify_output_kind(&step.output);
        let (connector, action) = extract_connector_action(step);
        let row = ExecutionStepRow {
            execution_id: execution_id.to_owned(),
            step_index: step.index as i64,
            step_kind: step.kind.clone(),
            connector,
            action,
            started_at: started,
            finished_at: Some(now),
            status: if step.error.is_some() {
                StepStatus::Failed
            } else {
                StepStatus::Succeeded
            },
            input_bytes: 0,
            output_bytes,
            output_kind: Some(output_kind.to_owned()),
            error_kind: step.error.as_ref().map(|e| classify_error_kind(e).to_owned()),
            input_blob_ref: None,
            output_blob_ref: None,
        };
        self.store
            .record_execution_step(row)
            .await
            .map_err(OperationError::from)?;
        Ok(())
    }

    async fn finish(
        &self,
        execution_id: &str,
        status: ExecutionStatus,
        error_kind: Option<&str>,
    ) -> Result<(), OperationError> {
        let finished_at = Self::now_ms();
        self.store
            .record_execution_finish(execution_id, status, error_kind, finished_at)
            .await
            .map_err(OperationError::from)?;
        Ok(())
    }
}

/// No-op recorder. Default when no store is wired. Drops every
/// call cleanly so test paths that don't care about persistence
/// stay short.
pub struct NoopRecorder;

#[async_trait]
impl ExecutionRecorder for NoopRecorder {
    async fn begin(
        &self,
        _execution: &ExecutionContext,
        _trigger_summary: &str,
        _recipe_id: Option<&str>,
    ) -> Result<(), OperationError> {
        Ok(())
    }
    async fn record_step(
        &self,
        _execution_id: &str,
        _step: &StepOutput,
    ) -> Result<(), OperationError> {
        Ok(())
    }
    async fn finish(
        &self,
        _execution_id: &str,
        _status: ExecutionStatus,
        _error_kind: Option<&str>,
    ) -> Result<(), OperationError> {
        Ok(())
    }
}

fn map_mode(mode: CoopExecutionMode) -> StoreExecutionMode {
    match mode {
        CoopExecutionMode::Cron => StoreExecutionMode::Cron,
        CoopExecutionMode::Webhook => StoreExecutionMode::Webhook,
        CoopExecutionMode::ConnectorEvent => StoreExecutionMode::ConnectorEvent,
        CoopExecutionMode::FileWatch => StoreExecutionMode::FileWatch,
        CoopExecutionMode::Manual => StoreExecutionMode::Manual,
        CoopExecutionMode::Cooperation => StoreExecutionMode::Cooperation,
        CoopExecutionMode::Retry => StoreExecutionMode::Retry,
        CoopExecutionMode::DryRun => StoreExecutionMode::DryRun,
    }
}

fn map_momentum(tier: MomentumTier) -> MomentumTag {
    match tier {
        MomentumTier::Cold => MomentumTag::Cold,
        MomentumTier::Warming => MomentumTag::Warming,
        MomentumTier::Hot => MomentumTag::Hot,
        MomentumTier::Fever => MomentumTag::Fever,
    }
}

/// Pull `connector` / `action` from a step's structured output when
/// the step was a connector call. Best-effort — the executions panel
/// uses these to render "connector-http : get" alongside the row.
fn extract_connector_action(step: &StepOutput) -> (Option<String>, Option<String>) {
    if step.kind != "run_connector" {
        return (None, None);
    }
    // The dispatcher's RunConnector arm wraps the connector's
    // ActionResult as `{ success, message, output }` — neither carries
    // the connector / action names directly. We rely on the
    // dispatcher to pass these via the step's `name` field when set,
    // and fall back to None otherwise. (The chain context already
    // has them via `chain.steps`, but the recorder doesn't.)
    // A future revision adds explicit connector/action on
    // StepOutput; today best-effort.
    (None, step.name.clone())
}

/// Map a step error message to a stable enum-tag for the audit
/// log. Privacy-default: full messages stay in `tracing` only;
/// the DB sees the tag.
fn classify_error_kind(msg: &str) -> &'static str {
    let lower = msg.to_ascii_lowercase();
    if lower.contains("timeout") || lower.contains("timed out") {
        "timeout"
    } else if lower.contains("refused") {
        "refused"
    } else if lower.contains("network") || lower.contains("connection") || lower.contains("dns") {
        "network"
    } else if lower.contains("selector") || lower.contains("element") {
        "selector_not_found"
    } else if lower.contains("schema") || lower.contains("invalid input") {
        "schema_invalid"
    } else if lower.contains("rate") || lower.contains("throttle") {
        "rate_limited"
    } else if lower.contains("permission") || lower.contains("forbidden") || lower.contains("unauthor") {
        "permission_denied"
    } else if lower.contains("sentinel") {
        "sentinel"
    } else {
        "unknown"
    }
}

/// Coarse-grained classification of a step output's content shape.
/// Recorded so the executions panel can render the right placeholder
/// when content isn't retained (sizes-only). We deliberately don't
/// inspect values past the top-level JSON type — the panel doesn't
/// need more.
fn classify_output_kind(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "bool",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "text",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "json",
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn error_kind_classification_buckets_common_messages() {
        assert_eq!(classify_error_kind("operation timed out"), "timeout");
        assert_eq!(classify_error_kind("connection refused"), "refused");
        assert_eq!(classify_error_kind("DNS lookup failed"), "network");
        assert_eq!(classify_error_kind("invalid CSS selector"), "selector_not_found");
        assert_eq!(classify_error_kind("schema validation failed"), "schema_invalid");
        assert_eq!(classify_error_kind("rate limit hit"), "rate_limited");
        assert_eq!(classify_error_kind("forbidden 403"), "permission_denied");
        assert_eq!(classify_error_kind("sentinel paused: foo"), "sentinel");
        assert_eq!(classify_error_kind("something else weird"), "unknown");
    }

    #[test]
    fn output_kind_classifies_top_level_shape() {
        assert_eq!(classify_output_kind(&serde_json::json!(null)), "null");
        assert_eq!(classify_output_kind(&serde_json::json!(true)), "bool");
        assert_eq!(classify_output_kind(&serde_json::json!(42)), "number");
        assert_eq!(classify_output_kind(&serde_json::json!("hi")), "text");
        assert_eq!(classify_output_kind(&serde_json::json!([])), "array");
        assert_eq!(classify_output_kind(&serde_json::json!({})), "json");
    }

    #[tokio::test]
    async fn noop_recorder_drops_every_call() {
        let recorder = NoopRecorder;
        let exec_ctx = ExecutionContext::for_global(
            springtale_core::rule::types::RuleId::new(),
            CoopExecutionMode::Manual,
        );
        recorder.begin(&exec_ctx, "test", None).await.unwrap();
        let step = StepOutput {
            index: 1,
            kind: "send_message".into(),
            name: None,
            output: serde_json::json!({ "text": "hi" }),
            duration_ms: 5,
            error: None,
        };
        recorder.record_step("01HXTEST", &step).await.unwrap();
        recorder
            .finish("01HXTEST", ExecutionStatus::Succeeded, None)
            .await
            .unwrap();
    }
}
