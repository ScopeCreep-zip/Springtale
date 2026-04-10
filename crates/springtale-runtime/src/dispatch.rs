//! Shared action dispatch — executes matched rule actions.
//!
//! Both springtaled (daemon) and springtale-bot call this module.
//! Single source of truth for action execution logic: RunConnector,
//! WriteFile (with path validation), Chain (with depth limits),
//! SendMessage, Notify, RunShell, Delay, Transform, AiComplete.
//!
//! Per architecture doc §6: "Build pipeline from Rule.actions,
//! each action becomes a pipeline Stage."

use std::sync::Arc;

use springtale_connector::registry::store::ConnectorRegistry;
use springtale_core::rule::action::Action;
use tokio::sync::RwLock;

/// Maximum size for WriteFile action content (10 MiB).
const MAX_WRITE_FILE_BYTES: usize = 10 * 1024 * 1024;

/// Dispatch a single action (entry point).
///
/// Boxed future for recursion support in Chain actions.
/// Depth tracking prevents infinite Chain recursion (max: `MAX_CHAIN_DEPTH`).
pub fn dispatch_action<'a>(
    action: &'a Action,
    registry: &'a Arc<RwLock<ConnectorRegistry>>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, String>> + Send + 'a>> {
    dispatch_with_depth(action, registry, 0)
}

fn dispatch_with_depth<'a>(
    action: &'a Action,
    registry: &'a Arc<RwLock<ConnectorRegistry>>,
    depth: u32,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, String>> + Send + 'a>> {
    Box::pin(dispatch_inner(action, registry, depth))
}

async fn dispatch_inner(
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
            // lock before the actual network call.
            let (host, checker) = {
                let reg = registry.read().await;
                reg.get_for_execute(connector).map_err(|e| e.to_string())?
            };

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
            tracing::info!(title = %title, body = %body, "NOTIFICATION");
            Ok(format!("notified: {title}"))
        }

        Action::SendMessage { text } => {
            tracing::info!(text = %text, "SendMessage (no destination context)");
            Ok(format!("message: {text}"))
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
            // Path validation — prevent arbitrary filesystem writes
            let path = std::path::Path::new(destination);
            if !path.is_absolute() {
                return Err("WriteFile requires absolute path".to_string());
            }
            if path
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
            {
                return Err("WriteFile path must not contain '..'".to_string());
            }
            tokio::fs::write(destination, content)
                .await
                .map_err(|e| format!("failed to write file {destination}: {e}"))?;
            tracing::info!(path = %destination, "file written");
            Ok(format!("wrote {destination}"))
        }

        Action::RunShell { command } => {
            // ShellExec requires capability approval flow.
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
                match dispatch_with_depth(step, registry, new_depth).await {
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
            tracing::debug!(operation = %operation, "transform pass-through");
            Ok(format!("transform: {operation}"))
        }

        Action::AiComplete { prompt, .. } => {
            tracing::debug!(
                prompt_len = prompt.len(),
                "AI complete pass-through (NoopAdapter)"
            );
            Ok("ai: noop".to_owned())
        }
    }
}
