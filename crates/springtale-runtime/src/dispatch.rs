//! Shared rule-action dispatcher — executes matched rule actions.
//!
//! Both springtaled (daemon) and springtale-bot route action dispatch
//! through this module. Per ARCHITECTURE.md §6.10, this is the single
//! enforcement point that calls `sentinel.evaluate()` before every
//! action.
//!
//! ## Phase 0 rework (chain context + real I/O)
//!
//! Pre-Phase-0 this module returned `Result<String, String>` and
//! discarded step outputs between iterations. `Action::AiComplete`
//! was a stub returning `"ai: noop"`. `Action::RunConnector` captured
//! only the connector's plain-text `message` and dropped its
//! structured `output: Value`. The net effect: ~12 shipped builtin
//! recipes referenced `${last_ai_output}` / `${last_connector_output}`
//! in their TOML, but the dispatcher had no path that resolved those
//! placeholders — users received literal `${last_ai_output}` strings
//! in their messaging channels.
//!
//! The new shape closes all four gaps:
//!
//! 1. Return type is `Result<ChainContext, ChainError>` —
//!    `ChainContext` carries every step's typed `output`, the
//!    `last_*_output` aliases, and the trigger payload. Callers read
//!    it to surface results or persist to the executions log.
//! 2. Before each step runs, action parameters are template-resolved
//!    against the chain via [`resolve_chain_value`] — `${trigger.x}`,
//!    `${last_ai_output}`, `${stepN.field}`, `${step.NAME.field}` all
//!    bind to live values.
//! 3. `RunConnector` captures the connector's `ActionResult.output`
//!    (the structured JSON) into `StepOutput.output`, not just the
//!    human message.
//! 4. `AiComplete` calls the real adapter via
//!    [`CapabilityBridge::ai_adapter_for`] — falls back to
//!    `NoopAdapter` (clean error, not silent stubbing) when no
//!    adapter is wired.
//!
//! Cooperation alignment: every dispatch carries an
//! [`ExecutionContext`] from `springtale-cooperation::execution`, so
//! the runtime knows which agent in which formation at which momentum
//! tier is firing. The bridge consults the tier for capability
//! routing (per-tier WASM `InstancePre` selection, §16). The sentinel
//! consults the tier for rate-budget scaling: Cold = 1/30s, Warming =
//! 12/min, Hot = 60/min, Fever = 600/min — the Phase 0.5 mapping in
//! [`crate::cooperation::momentum_to_throttle_tier`].

use std::sync::Arc;

use springtale_ai::{AiOptions, AiRequest};
use springtale_cooperation::execution::ExecutionContext;
use springtale_core::rule::action::Action;
use springtale_core::rule::template_resolve::{resolve_chain_template, resolve_chain_value};
use springtale_core::rule::{ChainContext, ChainError, StepOutput};
use springtale_sentinel::impact::ActionHints;
use springtale_sentinel::sentinel::EvaluateRequest;
use springtale_sentinel::{Sentinel, Verdict};

use crate::cooperation::{CapabilityBridge, momentum_to_throttle_tier, momentum_to_wasm_tier};

/// Maximum size for WriteFile action content (10 MiB).
const MAX_WRITE_FILE_BYTES: usize = 10 * 1024 * 1024;

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

/// Run one action against the chain. Recursive — `Action::Chain`
/// expands into multiple sub-steps that all share the chain context.
fn run_step<'a>(
    action: &'a Action,
    bridge: &'a CapabilityBridge,
    sentinel: &'a Arc<Sentinel>,
    execution: &'a ExecutionContext,
    chain: &'a mut ChainContext,
    depth: u32,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ChainError>> + Send + 'a>> {
    Box::pin(run_step_inner(
        action, bridge, sentinel, execution, chain, depth,
    ))
}

async fn run_step_inner(
    action: &Action,
    bridge: &CapabilityBridge,
    sentinel: &Arc<Sentinel>,
    execution: &ExecutionContext,
    chain: &mut ChainContext,
    depth: u32,
) -> Result<(), ChainError> {
    let run_id = execution.execution_id.to_string();

    // ── Sentinel ─────────────────────────────────────────────────
    // Throw the (unresolved) action at sentinel for connector-name
    // routing only — the resolver below produces the executable
    // version. Sentinel doesn't read action parameters; it reads the
    // connector name, the manifest's advisory hints for the named
    // action, and the envelope's policy / autonomy.
    let connector_name = match action {
        Action::RunConnector { connector, .. } => connector.as_str(),
        _ => "system",
    };
    let throttle_tier = momentum_to_throttle_tier(execution.momentum);
    let hints = if let Action::RunConnector {
        connector,
        action: name,
        ..
    } = action
    {
        let reg = bridge.registry().read().await;
        reg.get(connector)
            .and_then(|e| e.host.actions().iter().find(|d| d.name == *name).cloned())
            .map(|d| ActionHints {
                read_only: d.read_only,
                destructive: d.destructive,
            })
    } else {
        None
    };
    let action_name = if let Action::RunConnector { action: name, .. } = action {
        Some(name.as_str())
    } else {
        None
    };
    let verdict = sentinel
        .evaluate(EvaluateRequest {
            action,
            connector_name,
            tier: throttle_tier,
            hints,
            action_name,
            policy: execution.policy,
            autonomy: execution.autonomy,
        })
        .await;
    match verdict {
        Verdict::Go => {}
        Verdict::Throttle(duration) => {
            tracing::info!(
                connector = connector_name,
                delay_ms = duration.as_millis() as u64,
                "sentinel: throttling action"
            );
            tokio::time::sleep(duration).await;
        }
        Verdict::Pause(reason) => {
            return Err(ChainError::StepFailed {
                index: chain.next_step_index(),
                kind: action_kind(action).into(),
                message: format!("sentinel paused: {reason}"),
            });
        }
        Verdict::Quarantine(reason) => {
            return Err(ChainError::StepFailed {
                index: chain.next_step_index(),
                kind: action_kind(action).into(),
                message: format!("sentinel quarantined: {reason}"),
            });
        }
    }

    let kind = action_kind(action);
    let started = std::time::Instant::now();
    let dry_run = matches!(
        execution.mode,
        springtale_cooperation::execution::ExecutionMode::DryRun
    );

    // ── Action arm dispatch ─────────────────────────────────────
    let outcome: Result<StepOutput, ChainError> = match action {
        Action::RunConnector {
            connector,
            action: action_name,
            params,
        } => {
            // Resolve `${trigger.*}` / `${last_*_output.*}` /
            // `${stepN.*}` in every param string before handing to
            // the connector.
            let raw = serde_json::Value::Object(params.clone());
            let resolved = resolve_chain_value(&raw, chain, Some(&run_id));
            let input = resolved;

            // Dry-run stubs side-effecting connector actions but
            // lets read-only actions (HTTP get, browser navigate,
            // extract_text, etc.) run for real — that's the whole
            // point of "Test This Step": fetch real upstream data
            // to validate downstream rendering without spamming
            // the destination channel.
            if dry_run && is_side_effecting_action(action_name) {
                tracing::info!(
                    connector = %connector,
                    action = %action_name,
                    "DRY RUN — side-effecting connector action stubbed"
                );
                let step = StepOutput {
                    index: chain.next_step_index(),
                    kind: kind.into(),
                    name: None,
                    output: serde_json::json!({
                        "success": true,
                        "message": format!(
                            "dry-run: would call {connector}.{action_name}"
                        ),
                        "output": {
                            "connector": connector,
                            "action": action_name,
                            "params": input,
                        },
                        "dry_run": true,
                    }),
                    duration_ms: started.elapsed().as_millis() as u64,
                    error: None,
                };
                chain.record_step(step);
                sentinel.report_success(connector_name);
                return Ok(());
            }

            let effective_tier = momentum_to_wasm_tier(execution.momentum);
            let exec = bridge
                .execute(connector, action_name, input, effective_tier)
                .await;
            match exec {
                Ok(result) => {
                    tracing::info!(
                        connector = %connector,
                        action = %action_name,
                        success = result.success,
                        "connector action executed"
                    );
                    let index = chain.next_step_index();
                    // Capture both the structured output AND the
                    // human message so downstream templates can read
                    // either. `output` keys exposed in the chain
                    // alias: `last_connector_output.output.*` is the
                    // structured data, `last_connector_output.message`
                    // is the plain-text result.
                    let payload = serde_json::json!({
                        "success": result.success,
                        "message": result.message,
                        "output": result.output,
                    });
                    Ok(StepOutput {
                        index,
                        kind: kind.into(),
                        name: None,
                        output: payload,
                        duration_ms: started.elapsed().as_millis() as u64,
                        error: None,
                    })
                }
                Err(e) => {
                    tracing::warn!(
                        connector = %connector,
                        action = %action_name,
                        error = %e,
                        "connector action failed"
                    );
                    Err(ChainError::StepFailed {
                        index: chain.next_step_index(),
                        kind: kind.into(),
                        message: e.to_string(),
                    })
                }
            }
        }

        Action::Notify { title, body } => {
            let resolved_title = resolve_chain_template(title, chain, Some(&run_id));
            let resolved_body = resolve_chain_template(body, chain, Some(&run_id));
            if dry_run {
                tracing::info!(
                    title = %resolved_title,
                    "DRY RUN — Notify stubbed"
                );
            } else {
                tracing::info!(title = %resolved_title, body = %resolved_body, "NOTIFICATION");
            }
            Ok(StepOutput {
                index: chain.next_step_index(),
                kind: kind.into(),
                name: None,
                output: serde_json::json!({
                    "title": resolved_title,
                    "body": resolved_body,
                    "dry_run": dry_run,
                }),
                duration_ms: started.elapsed().as_millis() as u64,
                error: None,
            })
        }

        Action::SendMessage { text } => {
            let resolved = resolve_chain_template(text, chain, Some(&run_id));
            if dry_run {
                tracing::info!(text_len = resolved.len(), "DRY RUN — SendMessage stubbed");
            } else {
                tracing::info!(text = %resolved, "SendMessage (no destination context)");
            }
            Ok(StepOutput {
                index: chain.next_step_index(),
                kind: kind.into(),
                name: None,
                output: serde_json::json!({
                    "text": resolved,
                    "dry_run": dry_run,
                }),
                duration_ms: started.elapsed().as_millis() as u64,
                error: None,
            })
        }

        Action::WriteFile {
            destination,
            content,
            delete_source: _,
        } => {
            let resolved_destination = resolve_chain_template(destination, chain, Some(&run_id));
            let resolved_content = resolve_chain_template(content, chain, Some(&run_id));

            if resolved_content.len() > MAX_WRITE_FILE_BYTES {
                return Err(ChainError::StepFailed {
                    index: chain.next_step_index(),
                    kind: kind.into(),
                    message: format!(
                        "file content size ({} bytes) exceeds maximum ({MAX_WRITE_FILE_BYTES} bytes)",
                        resolved_content.len()
                    ),
                });
            }
            let path = std::path::Path::new(&resolved_destination);
            if !path.is_absolute() {
                return Err(ChainError::StepFailed {
                    index: chain.next_step_index(),
                    kind: kind.into(),
                    message: "WriteFile requires absolute path".to_string(),
                });
            }
            if path
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
            {
                return Err(ChainError::StepFailed {
                    index: chain.next_step_index(),
                    kind: kind.into(),
                    message: "WriteFile path must not contain '..'".to_string(),
                });
            }
            if dry_run {
                tracing::info!(
                    path = %resolved_destination,
                    bytes = resolved_content.len(),
                    "DRY RUN — WriteFile stubbed"
                );
            } else {
                tokio::fs::write(&resolved_destination, &resolved_content)
                    .await
                    .map_err(|e| ChainError::StepFailed {
                        index: chain.next_step_index(),
                        kind: kind.into(),
                        message: format!("failed to write file {resolved_destination}: {e}"),
                    })?;
                tracing::info!(path = %resolved_destination, "file written");
            }
            Ok(StepOutput {
                index: chain.next_step_index(),
                kind: kind.into(),
                name: None,
                output: serde_json::json!({
                    "path": resolved_destination,
                    "bytes": resolved_content.len(),
                    "dry_run": dry_run,
                }),
                duration_ms: started.elapsed().as_millis() as u64,
                error: None,
            })
        }

        Action::RunShell { command } => {
            let resolved = resolve_chain_template(command, chain, Some(&run_id));
            // ShellExec requires capability approval flow — actual
            // execution is gated outside the dispatcher. The
            // dispatcher records the request so the capability layer
            // and audit trail see it.
            tracing::info!(
                command = %resolved,
                "SHELL (not executed — requires ShellExec approval)"
            );
            Ok(StepOutput {
                index: chain.next_step_index(),
                kind: kind.into(),
                name: None,
                output: serde_json::json!({
                    "command": resolved,
                    "executed": false,
                    "reason": "ShellExec capability gate",
                }),
                duration_ms: started.elapsed().as_millis() as u64,
                error: None,
            })
        }

        Action::Delay { seconds } => {
            if dry_run {
                tracing::info!(seconds = seconds, "DRY RUN — Delay stubbed");
            } else {
                tokio::time::sleep(std::time::Duration::from_secs(*seconds)).await;
                tracing::debug!(seconds = seconds, "delay completed");
            }
            Ok(StepOutput {
                index: chain.next_step_index(),
                kind: kind.into(),
                name: None,
                output: serde_json::json!({ "seconds": seconds }),
                duration_ms: started.elapsed().as_millis() as u64,
                error: None,
            })
        }

        Action::Chain { steps } => {
            let new_depth = depth + 1;
            if new_depth > springtale_core::rule::action::MAX_CHAIN_DEPTH {
                return Err(ChainError::DepthExceeded {
                    depth: new_depth,
                    max: springtale_core::rule::action::MAX_CHAIN_DEPTH,
                });
            }
            // Chain expands transparently — each sub-step is recorded
            // as its own StepOutput in the shared ChainContext. The
            // Chain action itself doesn't produce a wrapper step.
            for (i, step) in steps.iter().enumerate() {
                match run_step(step, bridge, sentinel, execution, chain, new_depth).await {
                    Ok(()) => {}
                    Err(ChainError::Suppressed) => {
                        // A nested dedupe step suppressed the chain —
                        // propagate cleanly so the outer caller can
                        // mark execution status `empty`.
                        return Err(ChainError::Suppressed);
                    }
                    Err(e) => {
                        tracing::warn!(step = i, error = %e, "chain step failed");
                        return Err(e);
                    }
                }
            }
            // Chain returns without recording its own StepOutput —
            // sub-steps are already in chain.steps.
            //
            // Skip the post-step alias refresh path below: we already
            // returned the sub-steps individually.
            sentinel.report_success(connector_name);
            return Ok(());
        }

        Action::Transform { operation, params } => {
            // Transform is a placeholder today — operation-specific
            // implementations land in Phase A (the extraction ladder
            // replaces most Transform use cases). For now we record
            // the transform request so the chain context surfaces it.
            tracing::debug!(operation = %operation, "transform pass-through");
            Ok(StepOutput {
                index: chain.next_step_index(),
                kind: kind.into(),
                name: None,
                output: serde_json::json!({
                    "operation": operation,
                    "params": params,
                }),
                duration_ms: started.elapsed().as_millis() as u64,
                error: None,
            })
        }

        Action::AiComplete { prompt, adapter } => {
            // Resolve `${...}` placeholders in the prompt before the
            // model sees it. Critical: this is how `${last_connector_output}`
            // ends up in the prompt for "summarize this fetched
            // body" recipes.
            //
            // OWASP LLM01:2025 indirect-injection guard: every
            // substituted value is wrapped in `<external_context>` tags
            // by the AI-specific resolver, and the rule explaining the
            // tags is prepended to the system prompt. The model
            // therefore sees both (a) explicit instructions that the
            // tagged content is untrusted data, and (b) the tagged
            // values themselves.
            let resolved_user_prompt =
                springtale_core::rule::template_resolve::resolve_chain_template_for_ai(
                    prompt,
                    chain,
                    Some(&run_id),
                );
            let resolved_prompt = format!(
                "{rule}\n\n{prompt}",
                rule = springtale_core::rule::template_resolve::AI_EXTERNAL_CONTEXT_RULE,
                prompt = resolved_user_prompt,
            );

            // Route through the bridge — falls back to NoopAdapter
            // when no adapter is wired. NoopAdapter returns
            // `AiError::Disabled`, which we surface as a step error
            // (not a silent stub).
            let adapter_arc = bridge
                .ai_adapter_for(execution.agent_id.as_ref(), adapter.as_deref())
                .await;
            let request = AiRequest::Complete {
                prompt: resolved_prompt.clone(),
            };
            let options = AiOptions::default();
            let response = adapter_arc.complete(request, options).await;
            match response {
                Ok(ai_response) => {
                    tracing::debug!(
                        prompt_len = resolved_prompt.len(),
                        content_len = ai_response.content.len(),
                        finish_reason = ?ai_response.finish_reason,
                        "AI complete"
                    );
                    Ok(StepOutput {
                        index: chain.next_step_index(),
                        kind: kind.into(),
                        name: None,
                        output: serde_json::json!({
                            "text": ai_response.content,
                            "finish_reason": ai_response.finish_reason,
                        }),
                        duration_ms: started.elapsed().as_millis() as u64,
                        error: None,
                    })
                }
                Err(e) => {
                    tracing::warn!(error = %e, "AI complete failed");
                    Err(ChainError::StepFailed {
                        index: chain.next_step_index(),
                        kind: kind.into(),
                        message: e.to_string(),
                    })
                }
            }
        }

        Action::Extract {
            source,
            kind: extract_kind,
        } => {
            // Resolve `source` as a path against the chain — e.g.
            // `"last_connector_output.body"` → the HTTP body string,
            // or `"trigger.payload"` → the trigger event JSON.
            let resolved_source = resolve_chain_value(
                &serde_json::Value::String(format!("${{{source}}}")),
                chain,
                Some(&run_id),
            );

            // The AI adapter for LlmSchema extraction. We pass it
            // through opt-in — Phase A only fires non-LLM tiers;
            // Phase B activates LlmSchema and the adapter is read.
            let adapter_arc = bridge
                .ai_adapter_for(execution.agent_id.as_ref(), None)
                .await;
            let ai_ref: Option<&dyn springtale_ai::AiAdapter> = Some(&*adapter_arc);

            let extracted =
                crate::extraction::extract(&resolved_source, extract_kind, ai_ref).await;
            match extracted {
                Ok(value) => Ok(StepOutput {
                    index: chain.next_step_index(),
                    kind: kind.into(),
                    name: None,
                    output: value,
                    duration_ms: started.elapsed().as_millis() as u64,
                    error: None,
                }),
                Err(e) => Err(ChainError::StepFailed {
                    index: chain.next_step_index(),
                    kind: kind.into(),
                    message: e.to_string(),
                }),
            }
        }

        Action::Dedupe {
            key,
            bucket,
            history,
        } => {
            // Resolve key + bucket templates against the chain.
            let resolved_key = resolve_chain_template(key, chain, Some(&run_id));
            let resolved_bucket = resolve_chain_template(bucket, chain, Some(&run_id));

            // Bridge holds the store handle. Test builds without a
            // store wired fall through to "fresh" (the default impl
            // on the StorageBackend trait) so dispatcher tests don't
            // need a real DB just to exercise non-dedupe arms.
            let formation_id = execution.formation_id.map(|f| f.0.to_string());
            let rule_id = execution.rule_id.0.to_string();

            // Dry-run: never write to the dedupe table. We want
            // Test This Step to render the downstream steps as
            // if the data were fresh — without polluting the
            // real dedupe state for the next production fire.
            let outcome = if dry_run {
                springtale_store::schema::dedupe::DedupeOutcome::Fresh
            } else {
                match bridge.store() {
                    Some(store) => crate::dedupe::check_and_record(
                        store,
                        formation_id.as_deref(),
                        &rule_id,
                        &resolved_bucket,
                        &resolved_key,
                        *history,
                    )
                    .await
                    .map_err(|e| ChainError::StepFailed {
                        index: chain.next_step_index(),
                        kind: kind.into(),
                        message: e.to_string(),
                    })?,
                    None => springtale_store::schema::dedupe::DedupeOutcome::Fresh,
                }
            };

            // SeenBefore short-circuits the chain. The Chain runner
            // arm above catches `ChainError::Suppressed` and ends
            // the execution cleanly with status `empty`.
            if matches!(
                outcome,
                springtale_store::schema::dedupe::DedupeOutcome::SeenBefore
            ) {
                tracing::info!(
                    rule = %rule_id,
                    bucket = %resolved_bucket,
                    "dedupe: key seen before — chain suppressed"
                );
                return Err(ChainError::Suppressed);
            }

            Ok(StepOutput {
                index: chain.next_step_index(),
                kind: kind.into(),
                name: None,
                output: serde_json::json!({
                    "outcome": "fresh",
                    "bucket": resolved_bucket,
                    "dry_run": dry_run,
                }),
                duration_ms: started.elapsed().as_millis() as u64,
                error: None,
            })
        }
    };

    // ── Record outcome + sentinel report ────────────────────────
    match outcome {
        Ok(step) => {
            chain.record_step(step);
            sentinel.report_success(connector_name);
            Ok(())
        }
        Err(e) => {
            sentinel.report_failure(connector_name);
            Err(e)
        }
    }
}

/// Stable kind tag the dispatcher writes into [`StepOutput::kind`].
/// Mirrors the [`Action`] variant discriminant so chain-context
/// readers can filter by kind without re-matching the original
/// variant.
fn action_kind(action: &Action) -> &'static str {
    match action {
        Action::RunConnector { .. } => "run_connector",
        Action::SendMessage { .. } => "send_message",
        Action::WriteFile { .. } => "write_file",
        Action::RunShell { .. } => "run_shell",
        Action::Notify { .. } => "notify",
        Action::Chain { .. } => "chain",
        Action::Transform { .. } => "transform",
        Action::Delay { .. } => "delay",
        Action::AiComplete { .. } => "ai_complete",
        Action::Extract { .. } => "extract",
        Action::Dedupe { .. } => "dedupe",
    }
}

/// Classify a connector action name as side-effecting. Used by the
/// DryRun dispatcher path: side-effecting actions are stubbed
/// (return a "would have done X" StepOutput); read-only actions
/// run for real so Test This Step shows realistic upstream data.
///
/// The heuristic is verb-prefix based — connector authors who add
/// new write actions just need to use a recognizable prefix.
/// First-party connectors today: `send_message`, `post_*`,
/// `write_*`, `create_*`, `delete_*`, `update_*`, `publish_*`,
/// `commit_*`, `push_*`, `react`, `dispatch`, `react_to_message`,
/// `set_*` (config writes). Read-side actions use `get_*`, `list_*`,
/// `read_*`, `fetch_*`, `search_*`, `query_*`, `wait_*`, plus the
/// browser primitives `navigate`, `evaluate`, `screenshot`,
/// `extract_text`, `get_html`, `query_all`, `fill_form`, `click`.
///
/// `fill_form` + `click` are ambiguous — they mutate page state but
/// don't reach external systems. We classify them as read-only
/// (false) so chained recipes like "navigate → fill_form → click →
/// extract_text" produce useful Test This Step output. Connectors
/// that ship truly destructive actions under those names should
/// rename them.
fn is_side_effecting_action(name: &str) -> bool {
    const WRITE_PREFIXES: &[&str] = &[
        "send_",
        "post_",
        "write_",
        "create_",
        "delete_",
        "remove_",
        "update_",
        "publish_",
        "commit_",
        "push_",
        "dispatch_",
        "set_",
        "ban_",
        "kick_",
        "mute_",
        "broadcast_",
        "react_",
        "reply_",
        "subscribe_",
        "unsubscribe_",
        "approve_",
        "deny_",
    ];
    const WRITE_EXACT: &[&str] = &[
        "send",
        "post",
        "write",
        "publish",
        "commit",
        "react",
        "react_to_message",
        "dispatch",
        "ban",
        "kick",
        "mute",
    ];
    if WRITE_EXACT.contains(&name) {
        return true;
    }
    WRITE_PREFIXES.iter().any(|p| name.starts_with(p))
}

#[cfg(test)]
mod side_effect_tests {
    use super::is_side_effecting_action;

    #[test]
    fn send_message_is_side_effecting() {
        assert!(is_side_effecting_action("send_message"));
    }

    #[test]
    fn get_is_read_only() {
        assert!(!is_side_effecting_action("get"));
        assert!(!is_side_effecting_action("get_html"));
        assert!(!is_side_effecting_action("list_repos"));
    }

    #[test]
    fn browser_navigation_is_read_only() {
        assert!(!is_side_effecting_action("navigate"));
        assert!(!is_side_effecting_action("evaluate"));
        assert!(!is_side_effecting_action("screenshot"));
        assert!(!is_side_effecting_action("query_all"));
        assert!(!is_side_effecting_action("wait_for_selector"));
        assert!(!is_side_effecting_action("extract_text"));
    }

    #[test]
    fn write_prefixes_are_side_effecting() {
        for name in [
            "post_status",
            "write_file",
            "create_issue",
            "delete_message",
            "update_repo",
            "publish_release",
            "commit_change",
            "push_branch",
            "set_config",
            "ban_user",
            "kick_member",
            "mute_user",
        ] {
            assert!(
                is_side_effecting_action(name),
                "expected {name} to be side-effecting"
            );
        }
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
