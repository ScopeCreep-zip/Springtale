//! W1.F — Approval-gate UX dispatcher.
//!
//! The sentinel's `ChannelApprovalGate` (in `springtale-sentinel`)
//! produces `PendingApproval` envelopes when a destructive action
//! needs a human decision. This module owns:
//!
//!   1. The `Mutex<HashMap<Uuid, oneshot::Sender<bool>>>` that keeps
//!      track of in-flight requests so the frontend can resolve them
//!      by id.
//!   2. The background task that drains the gate's receiver, emits
//!      an `approval-required` Tauri event to the frontend, and
//!      stashes the responder for later.
//!   3. The `respond_to_approval` Tauri command the frontend calls
//!      after the user clicks Approve / Deny.
//!
//! `DefaultDenyApprovalGate` still runs in CLI / headless surfaces
//! per `feedback_preflight_zero_to_live`: silently denying is the
//! safe default when no UI is available, but on desktop / dashboard
//! we have a way to ask, so we do.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, State};
use tauri_specta::Event;
use tokio::sync::{oneshot, Mutex};
use uuid::Uuid;

use springtale_sentinel::approval::{ApprovalRequest, ChannelApprovalGate, PendingApproval};

/// State the dispatcher task + `respond_to_approval` command share.
pub struct ApprovalDispatcher {
    pending: Mutex<HashMap<String, oneshot::Sender<bool>>>,
}

impl ApprovalDispatcher {
    pub fn new() -> Self {
        Self {
            pending: Mutex::new(HashMap::new()),
        }
    }
}

impl Default for ApprovalDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

/// Build the gate + spawn the dispatcher task in one call. The gate
/// is what the sentinel calls; the dispatcher routes its requests
/// through Tauri events to the frontend.
///
/// Returns the gate as `Arc<dyn ApprovalGate>` ready to hand to
/// `springtale_runtime::init(...)`.
pub fn install(
    app: AppHandle,
    dispatcher: Arc<ApprovalDispatcher>,
    timeout: std::time::Duration,
) -> Arc<dyn springtale_sentinel::ApprovalGate> {
    let (gate, mut rx) = ChannelApprovalGate::new(timeout);
    let app_for_task = app.clone();
    tokio::spawn(async move {
        while let Some(PendingApproval { request, respond }) = rx.recv().await {
            let request_id = Uuid::new_v4().to_string();
            let payload = ApprovalRequired::from(&request_id, &request);
            // Store the responder *before* emitting so the frontend
            // can't beat us to `respond_to_approval`.
            dispatcher.pending.lock().await.insert(request_id.clone(), respond);
            if let Err(e) = payload.emit(&app_for_task) {
                tracing::warn!(error = %e, "approval-required event emit failed");
                // The frontend can't see this request — surface the
                // failure by removing the entry and resolving Deny so
                // the sentinel returns Quarantine.
                if let Some(resp) = dispatcher.pending.lock().await.remove(&request_id) {
                    let _ = resp.send(false);
                }
            }
        }
    });
    Arc::new(gate)
}

/// Frontend-facing event payload.
///
/// `tauri_specta::Event` derive surfaces this as a typed
/// `events.approvalRequired.listen(cb)` in the generated
/// `bindings.ts`. The string event name (`approval-required`) is
/// preserved via specta's snake-case → kebab-case event-name mapping;
/// the frontend never types the literal event name.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct ApprovalRequired {
    pub request_id: String,
    pub connector_name: String,
    pub action_type: String,
    pub rationale: String,
}

impl ApprovalRequired {
    fn from(request_id: &str, req: &ApprovalRequest) -> Self {
        Self {
            request_id: request_id.to_owned(),
            connector_name: req.connector_name.clone(),
            action_type: req.action_type.clone(),
            rationale: req.rationale.clone(),
        }
    }
}

/// Frontend calls this after the user clicks Approve or Deny in the
/// `ApprovalCard` overlay. Pops the keyed oneshot; the sentinel that
/// was awaiting wakes up with the decision.
#[tauri::command]
#[specta::specta]
pub async fn respond_to_approval(
    dispatcher: State<'_, Arc<ApprovalDispatcher>>,
    request_id: String,
    approve: bool,
) -> Result<(), String> {
    let mut pending = dispatcher.pending.lock().await;
    match pending.remove(&request_id) {
        Some(responder) => {
            let _ = responder.send(approve);
            Ok(())
        }
        None => Err(format!("no pending approval for id {request_id}")),
    }
}
