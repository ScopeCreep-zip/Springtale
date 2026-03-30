//! Action dispatcher — executes matched rule actions.
//!
//! This module bridges the gap between the rule engine (which matches triggers
//! to rules) and the connector/system layer (which performs the actual work).
//! Per architecture doc §6 line 1292: "Build pipeline from Rule.actions,
//! each action becomes a pipeline Stage."
//!
//! Phase 1a dispatches actions directly (no pipeline stages yet).
//! Phase 2 will wrap actions in pipeline stages with retry and fuel metering.

use std::sync::Arc;

use springtale_connector::registry::store::ConnectorRegistry;
use springtale_core::rule::action::Action;
use tokio::sync::RwLock;

/// Maximum size for WriteFile action content (10 MiB).
/// Prevents disk exhaustion from rules writing large files.
const MAX_WRITE_FILE_BYTES: usize = 10 * 1024 * 1024;

/// Dispatch a single action.
///
/// Called by the job consumer when a job is dequeued. The job payload
/// is a serialized `Action` which is deserialized and dispatched here.
/// Dispatch a single action (boxed for recursion in Chain).
///
/// Depth tracking prevents infinite Chain recursion. Max depth is
/// `MAX_CHAIN_DEPTH` (4) from springtale_core::rule::action.
pub fn dispatch_action<'a>(
    action: &'a Action,
    registry: &'a Arc<RwLock<ConnectorRegistry>>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, String>> + Send + 'a>> {
    dispatch_action_with_depth(action, registry, 0)
}

fn dispatch_action_with_depth<'a>(
    action: &'a Action,
    registry: &'a Arc<RwLock<ConnectorRegistry>>,
    depth: u32,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, String>> + Send + 'a>> {
    Box::pin(dispatch_action_inner(action, registry, depth))
}

async fn dispatch_action_inner(
    action: &Action,
    registry: &Arc<RwLock<ConnectorRegistry>>,
    depth: u32,
) -> Result<String, String> {
    match action {
        Action::RunConnector {
            connector,
            action: action_name,
            params,
        } => {
            let input = serde_json::Value::Object(params.clone());

            // Get Arc'd host + cloned capability checker under lock, then drop
            // lock before the actual network call. This prevents holding the
            // registry read lock across potentially long connector operations.
            let (host, checker) = {
                let reg = registry.read().await;
                reg.get_for_execute(connector).map_err(|e| e.to_string())?
            };
            // Lock is dropped here.

            match host.execute_checked(action_name, input, &checker).await {
                Ok(result) => {
                    tracing::info!(
                        connector = %connector,
                        action = %action_name,
                        success = result.success,
                        "connector action executed"
                    );
                    Ok(result.message)
                }
                Err(e) => {
                    tracing::warn!(
                        connector = %connector,
                        action = %action_name,
                        error = %e,
                        "connector action failed"
                    );
                    Err(e.to_string())
                }
            }
        }

        Action::Notify { title, body } => {
            // Phase 1a: log notification. Phase 2 adds notification channels.
            tracing::info!(title = %title, body = %body, "NOTIFICATION");
            Ok(format!("notified: {title}"))
        }

        Action::SendMessage { text } => {
            // Phase 1b: adds chat connectors. Phase 1a just logs.
            tracing::info!(text = %text, "MESSAGE");
            Ok(format!("message sent: {text}"))
        }

        Action::WriteFile {
            destination,
            content,
            delete_source: _,
        } => {
            if content.len() > MAX_WRITE_FILE_BYTES {
                return Err(format!(
                    "file content size ({} bytes) exceeds maximum ({MAX_WRITE_FILE_BYTES} bytes)",
                    content.len()
                ));
            }
            tokio::fs::write(destination, content)
                .await
                .map_err(|e| format!("failed to write file {destination}: {e}"))?;
            tracing::info!(path = %destination, "file written");
            Ok(format!("wrote {destination}"))
        }

        Action::RunShell { command } => {
            // Phase 1a: log only. ShellExec requires capability approval flow.
            tracing::info!(command = %command, "SHELL (not executed — requires ShellExec approval)");
            Ok(format!("shell logged: {command}"))
        }

        Action::Delay { seconds } => {
            tokio::time::sleep(std::time::Duration::from_secs(*seconds)).await;
            tracing::debug!(seconds = seconds, "delay completed");
            Ok(format!("delayed {seconds}s"))
        }

        Action::Chain { steps } => {
            let new_depth = depth + 1;
            if new_depth > springtale_core::rule::action::MAX_CHAIN_DEPTH {
                return Err(format!(
                    "chain depth {new_depth} exceeds max {}",
                    springtale_core::rule::action::MAX_CHAIN_DEPTH
                ));
            }

            let mut results = Vec::new();
            for (i, step) in steps.iter().enumerate() {
                match dispatch_action_with_depth(step, registry, new_depth).await {
                    Ok(msg) => results.push(msg),
                    Err(e) => {
                        tracing::warn!(step = i, error = %e, "chain step failed");
                        return Err(format!("chain step {i} failed: {e}"));
                    }
                }
            }
            Ok(format!("chain completed: {} steps", results.len()))
        }

        Action::Transform { operation, .. } => {
            // Phase 2: adds transform operations (extract, format, filter).
            tracing::debug!(operation = %operation, "transform pass-through");
            Ok(format!("transform: {operation}"))
        }

        Action::AiComplete { prompt, .. } => {
            // Phase 2a: adds real AI adapters. NoopAdapter passes through.
            tracing::debug!(prompt_len = prompt.len(), "AI complete pass-through (NoopAdapter)");
            Ok("ai: noop".to_owned())
        }
    }
}
