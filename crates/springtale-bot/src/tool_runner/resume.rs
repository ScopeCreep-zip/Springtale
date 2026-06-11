//! W2 boot resumer — replay chat threads that were paused behind an
//! approval when the daemon restarted (2026 durable-resume pattern:
//! the checkpoint row IS the thread's pending interrupt).
//!
//! For each orphaned checkpoint, join back to its approval verdict by the
//! bound-action summary (`{connector}.{action}` — the same string the
//! gate records) at/after the checkpoint instant:
//! - **Approved** → execute exactly the persisted bound calls (OWASP
//!   Agentic: never a re-derived action), feed results into the saved
//!   transcript, and continue `run_with_tools` to completion; deliver the
//!   final text to the origin channel.
//! - **Denied / timed out** → notify the origin channel, drop the thread.
//! - **Still pending** → poll until the verdict or expiry lands (the
//!   approval row's own expiry keeps this bounded).

use std::sync::Arc;
use std::time::Duration;

use springtale_ai::{AiAdapter, AiOptions, ChatMessage, ToolCall, ToolPolicy};
use springtale_connector::registry::store::ConnectorRegistry;
use springtale_runtime::CapabilityBridge;
use springtale_store::{StorageBackend, ToolLoopCheckpointRow};
use tokio::sync::RwLock;

use super::loop_::{ToolRunnerCall, ToolRunnerDeps, run_with_tools};
use super::split_tool_name;
use crate::runtime::lifecycle::OutgoingResponse;

/// Handles the resumer needs, cloned off `Bot` at event-loop start so the
/// task owns its world.
pub struct ResumerDeps {
    pub store: Arc<dyn StorageBackend>,
    pub registry: Arc<RwLock<ConnectorRegistry>>,
    pub bridge: CapabilityBridge,
    pub sentinel: Arc<springtale_sentinel::Sentinel>,
    pub adapter: Arc<dyn AiAdapter>,
    pub response_tx: tokio::sync::mpsc::Sender<OutgoingResponse>,
    pub policy: ToolPolicy,
}

/// How often a still-pending verdict is re-checked. The approval's own
/// expiry bounds total wait; polling exists only for the post-restart case
/// where the gate's in-process waiter died with the old process.
const VERDICT_POLL: Duration = Duration::from_secs(15);

pub async fn resume_orphaned_loops(deps: ResumerDeps) {
    let checkpoints = match deps.store.list_tool_loop_checkpoints().await {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!(error = %e, "resumer: checkpoint list failed");
            return;
        }
    };
    if checkpoints.is_empty() {
        return;
    }
    tracing::info!(
        count = checkpoints.len(),
        "resumer: replaying paused chat threads"
    );
    for cp in checkpoints {
        resume_one(&deps, cp).await;
    }
}

async fn resume_one(deps: &ResumerDeps, cp: ToolLoopCheckpointRow) {
    let tool_calls: Vec<ToolCall> = match serde_json::from_str(&cp.pending_tool_json) {
        Ok(v) => v,
        Err(_) => {
            let _ = deps
                .store
                .delete_tool_loop_checkpoint(&cp.session_key)
                .await;
            return;
        }
    };
    // The gate records summaries as `{connector}.{action}`; tool names are
    // `{connector}__{action}` — join on the first gated call's summary.
    let Some(summary) = tool_calls
        .first()
        .and_then(|c| split_tool_name(&c.name))
        .map(|(conn, act)| format!("{conn}.{act}"))
    else {
        let _ = deps
            .store
            .delete_tool_loop_checkpoint(&cp.session_key)
            .await;
        return;
    };

    // Wait (bounded by the approval's own expiry) for a verdict.
    let verdict = loop {
        match deps
            .store
            .find_approval_by_summary(&summary, cp.created_at)
            .await
        {
            Ok(Some(row)) => match row.decision_json {
                Some(d) => break d,
                None => tokio::time::sleep(VERDICT_POLL).await, // pending — card is live
            },
            // No matching approval row: the round was never gated (or the
            // row expired out) — nothing to wait on; treat as closed.
            Ok(None) => break String::from("{\"kind\":\"timed_out\"}"),
            Err(e) => {
                tracing::warn!(error = %e, "resumer: verdict lookup failed");
                return;
            }
        }
    };

    if !verdict.contains("approved") {
        notify(deps, &cp, "↩️ I restarted while waiting on that approval and it was denied or expired — nothing was run. Just ask again if you still want it.").await;
        let _ = deps
            .store
            .delete_tool_loop_checkpoint(&cp.session_key)
            .await;
        return;
    }

    // Approved: continue the loop from the persisted transcript. The bound
    // calls re-dispatch through the normal gate path — the still-valid
    // approval row satisfies it, and sentinel gating reapplies as usual.
    let messages: Vec<ChatMessage> = match serde_json::from_str(&cp.messages_json) {
        Ok(m) => m,
        Err(_) => {
            let _ = deps
                .store
                .delete_tool_loop_checkpoint(&cp.session_key)
                .await;
            return;
        }
    };
    let runner_deps = ToolRunnerDeps {
        adapter: deps.adapter.as_ref(),
        registry: &deps.registry,
        bridge: &deps.bridge,
        sentinel: &deps.sentinel,
    };
    let call = ToolRunnerCall {
        options: AiOptions::default(),
        policy: &deps.policy,
        formation_tier: None,
        checkpoint: Some(super::loop_::CheckpointCtx {
            session_key: cp.session_key.clone(),
            origin_connector: cp.origin_connector.clone(),
            origin_channel: cp.origin_channel.clone(),
        }),
    };
    match run_with_tools(runner_deps, messages, call).await {
        Ok(response) if !response.content.is_empty() => {
            notify(deps, &cp, &response.content).await;
        }
        Ok(_) => {
            notify(
                deps,
                &cp,
                "✅ Done (approved task completed after restart).",
            )
            .await
        }
        Err(e) => {
            tracing::warn!(error = %e, session = %cp.session_key, "resumer: replay failed");
            notify(
                deps,
                &cp,
                "I couldn't finish the approved task after restarting — please ask again.",
            )
            .await;
            let _ = deps
                .store
                .delete_tool_loop_checkpoint(&cp.session_key)
                .await;
        }
    }
}

async fn notify(deps: &ResumerDeps, cp: &ToolLoopCheckpointRow, text: &str) {
    let _ = deps
        .response_tx
        .send(OutgoingResponse {
            channel_id: cp.origin_channel.clone(),
            text: text.to_owned(),
            connector: cp.origin_connector.clone(),
        })
        .await;
}
