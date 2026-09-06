//! The `Action::RunConnector` arm — the only step that reaches a
//! connector, plus the read-only/side-effecting split that decides what a
//! dry run is allowed to actually do.

use springtale_cooperation::execution::ExecutionContext;
use springtale_core::rule::template_resolve::resolve_chain_value;
use springtale_core::rule::{ChainContext, ChainError, StepOutput};

use crate::cooperation::{CapabilityBridge, momentum_to_wasm_tier};
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
/// Run one connector action against the chain.
///
/// Returns the [`StepOutput`] the caller records; the dry-run path returns a
/// stub step rather than calling the connector.
#[allow(clippy::too_many_arguments)]
pub(super) async fn run_connector_step(
    connector: &str,
    action_name: &str,
    params: &serde_json::Map<String, serde_json::Value>,
    bridge: &CapabilityBridge,
    execution: &ExecutionContext,
    chain: &mut ChainContext,
    run_id: &str,
    kind: &'static str,
    started: std::time::Instant,
    dry_run: bool,
) -> Result<StepOutput, ChainError> {
    // Resolve `${trigger.*}` / `${last_*_output.*}` /
    // `${stepN.*}` in every param string before handing to
    // the connector.
    let raw = serde_json::Value::Object(params.clone());
    let resolved = resolve_chain_value(&raw, chain, Some(run_id));
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
        return Ok(step);
    }

    let effective_tier = momentum_to_wasm_tier(execution.momentum);
    let exec = bridge
        .execute_with_origin(
            connector,
            action_name,
            input,
            effective_tier,
            execution.origin.clone(),
        )
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
