//! The step runner — sentinel gate, template resolution, and the
//! per-`Action` arm dispatch that every chain step goes through.
//!
//! Arms with enough substance to stand alone live in sibling modules
//! ([`super::connector`], [`super::chain`], [`super::extract`]); the rest
//! are inline here because they are a handful of lines each.

use std::sync::Arc;

use springtale_ai::{AiOptions, AiRequest};
use springtale_cooperation::execution::ExecutionContext;
use springtale_core::rule::action::Action;
use springtale_core::rule::template_resolve::resolve_chain_template;
use springtale_core::rule::{ChainContext, ChainError, StepOutput};
use springtale_sentinel::impact::ActionHints;
use springtale_sentinel::sentinel::EvaluateRequest;
use springtale_sentinel::{Sentinel, Verdict};

use super::{chain, connector, extract};
use crate::cooperation::{CapabilityBridge, momentum_to_throttle_tier};

/// Maximum size for WriteFile action content (10 MiB).
const MAX_WRITE_FILE_BYTES: usize = 10 * 1024 * 1024;

/// Run one action against the chain. Recursive — `Action::Chain`
/// expands into multiple sub-steps that all share the chain context.
pub(super) fn run_step<'a>(
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
            origin: execution.origin.as_ref(),
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
            chain.throttles += 1;
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
            connector::run_connector_step(
                connector,
                action_name,
                params,
                bridge,
                execution,
                chain,
                &run_id,
                kind,
                started,
                dry_run,
            )
            .await
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
            chain::run_chain_steps(steps, bridge, sentinel, execution, chain, depth).await?;
            // Chain records no wrapper step of its own — its sub-steps are
            // already in `chain.steps`, so skip the post-step alias refresh.
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

        Action::AiComplete { prompt, .. } => {
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
            let adapter_arc = bridge.ai_adapter_for(execution).await;
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
            extract::run_extract_step(
                source,
                extract_kind,
                bridge,
                execution,
                chain,
                &run_id,
                kind,
                started,
            )
            .await
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
