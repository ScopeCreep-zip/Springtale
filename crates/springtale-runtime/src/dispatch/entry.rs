//! Dispatch entry points — what callers outside this crate reach for.
//!
//! [`dispatch_action`] and [`dispatch_actions`] own the chain-fire
//! envelope: they build the [`ChainContext`], hand each top-level action
//! to [`super::step::run_step`], and record the executions-log row. The
//! per-action work lives in the sibling modules.

use std::sync::Arc;

use springtale_cooperation::execution::ExecutionContext;
use springtale_core::rule::action::Action;
use springtale_core::rule::{ChainContext, ChainError};
use springtale_sentinel::Sentinel;

use super::step::run_step;
use crate::cooperation::CapabilityBridge;

/// Dispatch one top-level rule action with full chain-context
/// threading. Returns the final [`ChainContext`] containing every
/// recorded [`StepOutput`].
///
/// `trigger_payload` is the JSON the trigger fired with — referenced
/// by recipe templates as `${trigger.path}`. Cron triggers pass
/// `Value::Null`. Webhook / connector-event triggers pass the
/// inbound payload.
pub fn dispatch_action<'a>(
    action: &'a Action,
    bridge: &'a CapabilityBridge,
    sentinel: &'a Arc<Sentinel>,
    execution: ExecutionContext,
    trigger_payload: serde_json::Value,
) -> std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<ChainContext, ChainError>> + Send + 'a>,
> {
    dispatch_actions(
        std::slice::from_ref(action),
        bridge,
        sentinel,
        execution,
        trigger_payload,
    )
}
/// Dispatch a sequence of top-level actions as a single chain fire.
/// Used by `trigger_dispatch` when a [`RuleMatch::actions`] holds
/// `Vec<Action>` — each action becomes a step in the shared
/// `ChainContext`, so `${last_*_output}` and `${stepN.*}` resolve
/// across the whole rule.
pub fn dispatch_actions<'a>(
    actions: &'a [Action],
    bridge: &'a CapabilityBridge,
    sentinel: &'a Arc<Sentinel>,
    execution: ExecutionContext,
    trigger_payload: serde_json::Value,
) -> std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<ChainContext, ChainError>> + Send + 'a>,
> {
    Box::pin(async move {
        let recorder = bridge.recorder();
        let trigger_summary = summarize_trigger(&trigger_payload, execution.mode);
        // Best-effort recorder.begin — failures fall through to
        // dispatch so the chain still runs. The privacy invariant
        // is about NOT writing content; missing rows are fine.
        if let Err(e) = recorder.begin(&execution, &trigger_summary, None).await {
            tracing::warn!(error = %e, "executions recorder.begin failed");
        }

        let execution_id = execution.execution_id.to_string();
        let mut chain = ChainContext::new(trigger_payload);
        let mut steps_emitted = 0usize;
        let mut final_status = springtale_store::schema::executions::ExecutionStatus::Succeeded;
        let mut error_kind: Option<&'static str> = None;
        let mut chain_outcome: Result<(), ChainError> = Ok(());

        for action in actions {
            match run_step(action, bridge, sentinel, &execution, &mut chain, 0).await {
                Ok(()) => {
                    // Record any new steps the action appended to
                    // chain.steps. Chain action expands into
                    // multiple sub-steps so we drain everything past
                    // `steps_emitted`.
                    while steps_emitted < chain.steps.len() {
                        let step = &chain.steps[steps_emitted];
                        if let Err(e) = recorder.record_step(&execution_id, step).await {
                            tracing::warn!(error = %e, "executions recorder.record_step failed");
                        }
                        steps_emitted += 1;
                    }
                }
                Err(ChainError::Suppressed) => {
                    // Flush any steps that did run (dedupe step itself).
                    while steps_emitted < chain.steps.len() {
                        let step = &chain.steps[steps_emitted];
                        if let Err(e) = recorder.record_step(&execution_id, step).await {
                            tracing::warn!(error = %e, "executions recorder.record_step failed");
                        }
                        steps_emitted += 1;
                    }
                    final_status = springtale_store::schema::executions::ExecutionStatus::Empty;
                    chain_outcome = Ok(());
                    break;
                }
                Err(e) => {
                    while steps_emitted < chain.steps.len() {
                        let step = &chain.steps[steps_emitted];
                        if let Err(rec_err) = recorder.record_step(&execution_id, step).await {
                            tracing::warn!(error = %rec_err, "executions recorder.record_step failed");
                        }
                        steps_emitted += 1;
                    }
                    final_status = springtale_store::schema::executions::ExecutionStatus::Failed;
                    error_kind = Some(classify_chain_error(&e));
                    chain_outcome = Err(e);
                    break;
                }
            }
        }

        if let Err(e) = recorder
            .finish(&execution_id, final_status, error_kind)
            .await
        {
            tracing::warn!(error = %e, "executions recorder.finish failed");
        }

        match chain_outcome {
            Ok(()) => Ok(chain),
            Err(e) => Err(e),
        }
    })
}
/// Build a short summary string the executions log records for the
/// firing trigger. Sized for a status line — no payload, just the
/// kind + the obvious discriminator.
fn summarize_trigger(
    trigger: &serde_json::Value,
    mode: springtale_cooperation::execution::ExecutionMode,
) -> String {
    use springtale_cooperation::execution::ExecutionMode as M;
    match mode {
        M::Cron => trigger
            .get("expression")
            .and_then(|v| v.as_str())
            .map(|e| format!("Cron {e}"))
            .unwrap_or_else(|| "Cron".to_owned()),
        M::Webhook => "Webhook".to_owned(),
        M::ConnectorEvent => trigger
            .get("trigger_name")
            .and_then(|v| v.as_str())
            .map(|t| format!("Event {t}"))
            .unwrap_or_else(|| "ConnectorEvent".to_owned()),
        M::FileWatch => "FileWatch".to_owned(),
        M::Manual => "Manual".to_owned(),
        M::Cooperation => "Cooperation".to_owned(),
        M::Retry => "Retry".to_owned(),
        M::DryRun => "DryRun".to_owned(),
    }
}
/// Map a chain error to the same enum-tag set the recorder writes
/// for step errors. Keeps the audit trail consistent — privacy
/// invariant says no full messages reach the DB.
fn classify_chain_error(err: &ChainError) -> &'static str {
    match err {
        ChainError::Suppressed => "suppressed",
        ChainError::StepNotYetRun(_) => "template_step_unresolved",
        ChainError::StepNameNotFound(_) => "template_name_unresolved",
        ChainError::DuplicateStepName(_) => "template_duplicate_name",
        ChainError::DepthExceeded { .. } => "chain_depth_exceeded",
        ChainError::StepFailed { .. } => "step_failed",
        ChainError::Template(_) => "template_invalid",
    }
}
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use springtale_cooperation::execution::{
        ExecutionContext as CoopExecutionContext, ExecutionMode as CoopExecutionMode,
    };
    use springtale_store::SqliteBackend;
    use springtale_store::backend::StorageBackend;
    use springtale_store::schema::executions::ExecutionFilter;
    use std::sync::Arc;

    /// Build a bridge wired against an in-memory SqliteBackend with
    /// a real StoreRecorder — used by tests that assert on
    /// executions-log rows after a chain runs.
    fn bridge_with_recorded_store() -> (CapabilityBridge, Arc<dyn StorageBackend>) {
        let store: Arc<dyn StorageBackend> = Arc::new(SqliteBackend::open_in_memory().unwrap());
        let recorder: Arc<dyn crate::operations::executions::ExecutionRecorder> = Arc::new(
            crate::operations::executions::StoreRecorder::new(store.clone()),
        );
        let registry = Arc::new(tokio::sync::RwLock::new(
            springtale_connector::registry::store::ConnectorRegistry::default(),
        ));
        let bridge = CapabilityBridge::new(registry)
            .with_store(store.clone())
            .with_recorder(recorder);
        (bridge, store)
    }

    fn manual_execution_ctx() -> CoopExecutionContext {
        CoopExecutionContext::for_global(
            springtale_core::rule::types::RuleId::new(),
            CoopExecutionMode::Manual,
        )
    }

    #[tokio::test]
    async fn dispatch_records_execution_and_step_rows() {
        let (bridge, store) = bridge_with_recorded_store();
        let sentinel = Arc::new(springtale_sentinel::Sentinel::new(
            springtale_sentinel::SentinelConfig::default(),
            store.clone(),
        ));
        let execution = manual_execution_ctx();
        let exec_id = execution.execution_id.to_string();

        // SendMessage: simplest non-network action — produces one step.
        let action = Action::SendMessage {
            text: "hello".into(),
        };
        let chain = dispatch_action(
            &action,
            &bridge,
            &sentinel,
            execution,
            serde_json::Value::Null,
        )
        .await
        .unwrap();
        assert_eq!(chain.steps.len(), 1);

        // executions row recorded.
        let list = store
            .list_executions(ExecutionFilter::default())
            .await
            .unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, exec_id);
        assert_eq!(
            list[0].status,
            springtale_store::schema::executions::ExecutionStatus::Succeeded
        );
        assert_eq!(
            list[0].mode,
            springtale_store::schema::executions::ExecutionMode::Manual
        );

        // execution_steps row recorded with sizes only.
        let steps = store.get_execution_steps(&exec_id).await.unwrap();
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].step_kind, "send_message");
        assert!(steps[0].output_bytes > 0, "output_bytes captured size");
        assert!(
            steps[0].input_blob_ref.is_none() && steps[0].output_blob_ref.is_none(),
            "privacy default: no content retained"
        );
    }

    #[tokio::test]
    async fn dispatch_dry_run_stubs_sendmessage_and_returns_dry_run_flag() {
        let (bridge, store) = bridge_with_recorded_store();
        let sentinel = Arc::new(springtale_sentinel::Sentinel::new(
            springtale_sentinel::SentinelConfig::default(),
            store.clone(),
        ));
        let execution = CoopExecutionContext::for_global(
            springtale_core::rule::types::RuleId::new(),
            CoopExecutionMode::DryRun,
        );

        let action = Action::SendMessage {
            text: "would have sent this".into(),
        };
        let chain = dispatch_action(
            &action,
            &bridge,
            &sentinel,
            execution,
            serde_json::Value::Null,
        )
        .await
        .unwrap();

        assert_eq!(chain.steps.len(), 1);
        let step = &chain.steps[0];
        assert_eq!(step.kind, "send_message");
        assert_eq!(
            step.output.get("dry_run").and_then(|v| v.as_bool()),
            Some(true)
        );

        // Executions log captured the run in DryRun mode.
        let runs = store
            .list_executions(ExecutionFilter::default())
            .await
            .unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(
            runs[0].mode,
            springtale_store::schema::executions::ExecutionMode::DryRun
        );
    }

    #[tokio::test]
    async fn dispatch_records_failure_status_on_step_failure() {
        // WriteFile with a relative path is rejected at the
        // dispatcher's pre-flight — yields a StepFailed chain error.
        let (bridge, store) = bridge_with_recorded_store();
        let sentinel = Arc::new(springtale_sentinel::Sentinel::new(
            springtale_sentinel::SentinelConfig::default(),
            store.clone(),
        ));
        let execution = manual_execution_ctx();
        let exec_id = execution.execution_id.to_string();

        let action = Action::WriteFile {
            destination: "relative.txt".into(),
            content: "data".into(),
            delete_source: false,
        };
        let result = dispatch_action(
            &action,
            &bridge,
            &sentinel,
            execution,
            serde_json::Value::Null,
        )
        .await;
        assert!(result.is_err());

        let list = store
            .list_executions(ExecutionFilter::default())
            .await
            .unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(
            list[0].status,
            springtale_store::schema::executions::ExecutionStatus::Failed,
            "WriteFile rejected → executions row marked failed"
        );
        assert_eq!(list[0].error_kind.as_deref(), Some("step_failed"));
        let _ = exec_id; // unused but kept for parallel-with-success test
    }

    /// Minimal native connector whose single action declares no
    /// `destructive` hint — the "unknown hint" case the sentinel must
    /// treat as destructive (MCP `destructiveHint` default `true`).
    struct HintlessConnector {
        manifest: springtale_connector::manifest::types::ConnectorManifest,
    }

    impl HintlessConnector {
        fn new(name: &str) -> Self {
            use springtale_connector::manifest::SignatureAlgorithm;
            use springtale_connector::manifest::types::{
                ActionDecl, Capability, ConnectorManifest, TriggerDecl,
            };
            Self {
                manifest: ConnectorManifest {
                    name: name.to_owned(),
                    version: "0.1.0".into(),
                    author: "test".into(),
                    description: "hintless".into(),
                    capabilities: vec![Capability::NetworkOutbound {
                        host: "api.example.com".into(),
                    }],
                    triggers: vec![TriggerDecl {
                        name: "test_event".into(),
                        description: "test".into(),
                        schema: None,
                    }],
                    actions: vec![ActionDecl {
                        read_only: false,
                        destructive: None,
                        poll_interval_secs: None,
                        name: "echo".into(),
                        description: "echo".into(),
                        input_schema: None,
                        output_schema: None,
                    }],
                    data_disclosure: vec![],
                    roles: vec![],
                    wasm_hash: None,
                    signature_alg: SignatureAlgorithm::default(),
                    signature: None,
                },
            }
        }
    }

    #[async_trait::async_trait]
    impl springtale_connector::connector::trait_::Connector for HintlessConnector {
        fn triggers(&self) -> &[springtale_connector::manifest::types::TriggerDecl] {
            &self.manifest.triggers
        }
        fn actions(&self) -> &[springtale_connector::manifest::types::ActionDecl] {
            &self.manifest.actions
        }
        async fn execute(
            &self,
            action: &str,
            input: serde_json::Value,
        ) -> Result<
            springtale_connector::connector::trait_::ActionResult,
            springtale_connector::ConnectorError,
        > {
            Ok(springtale_connector::connector::trait_::ActionResult {
                success: true,
                output: serde_json::json!({"echoed": input, "action": action}),
                message: "ok".into(),
            })
        }
        async fn on_event(
            &self,
            trigger: &str,
            _handler: springtale_connector::connector::trait_::EventHandler,
        ) -> Result<
            springtale_connector::connector::subscription::Subscription,
            springtale_connector::ConnectorError,
        > {
            Ok(
                springtale_connector::connector::subscription::Subscription {
                    id: springtale_connector::connector::subscription::SubscriptionId(0),
                    trigger: trigger.to_owned(),
                },
            )
        }
        async fn remove_event(
            &self,
            _sub: &springtale_connector::connector::subscription::Subscription,
        ) -> Result<(), springtale_connector::ConnectorError> {
            Ok(())
        }
        fn manifest(&self) -> &springtale_connector::manifest::types::ConnectorManifest {
            &self.manifest
        }
    }

    #[tokio::test]
    async fn dispatch_quarantines_hintless_connector_action_under_default_deny() {
        let store: Arc<dyn StorageBackend> = Arc::new(SqliteBackend::open_in_memory().unwrap());
        let mut registry = springtale_connector::registry::store::ConnectorRegistry::new(
            springtale_connector::capability::grant::CapabilityPolicy::AllowAll,
        );
        registry
            .install_native(Box::new(HintlessConnector::new("hintless")))
            .unwrap();
        let bridge = CapabilityBridge::new(Arc::new(tokio::sync::RwLock::new(registry)))
            .with_store(store.clone());
        // `Sentinel::new` wires `DefaultDenyApprovalGate`.
        let sentinel = Arc::new(springtale_sentinel::Sentinel::new(
            springtale_sentinel::SentinelConfig::default(),
            store.clone(),
        ));
        let execution = manual_execution_ctx();

        let action = Action::RunConnector {
            connector: "hintless".into(),
            action: "echo".into(),
            params: serde_json::Map::new(),
        };
        let err = dispatch_action(
            &action,
            &bridge,
            &sentinel,
            execution,
            serde_json::Value::Null,
        )
        .await
        .unwrap_err();
        assert!(
            matches!(&err, ChainError::StepFailed { message, .. } if message.contains("quarantined")),
            "expected sentinel quarantine, got {err:?}"
        );
    }
}
