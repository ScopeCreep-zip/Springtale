//! Destructive-action approval gate.
//!
//! The sentinel's fourth check (after circuit-breaker, rate-limit,
//! dead-man) classifies the action via [`crate::impact::classify_impact`].
//! When the result is [`crate::impact::ActionImpact::Destructive`]
//! the sentinel routes the decision through an [`ApprovalGate`].
//!
//! Surfaces:
//! - **CLI / headless** — wire [`DefaultDenyApprovalGate`]. Safe
//!   default for the most vulnerable user: a destructive action
//!   without a human on the other end never runs. Survivors using
//!   `springtale run` from a terminal aren't ambushed by a runaway
//!   bot deleting their data.
//! - **Desktop / web dashboard** — wire a custom gate that emits a
//!   confirmation prompt to the frontend, awaits the user's
//!   decision (with a timeout fallback to deny), and returns the
//!   verdict. The runtime exposes a [`ChannelApprovalGate`] for
//!   exactly this — the desktop app constructs one, hands it to
//!   the sentinel, and listens on the receiver to dispatch each
//!   request to a Tauri event.
//! - **Tests** — [`AutoAllowApprovalGate`] removes the gate from
//!   the path, useful for asserting other sentinel checks.
//!
//! The trait is async because real implementations must await
//! either a network round-trip or a UI confirmation. Default impls
//! resolve synchronously; the async signature costs nothing on the
//! Go path.

use async_trait::async_trait;
use std::time::Duration;

/// Single approval request — what the gate sees.
#[derive(Debug, Clone)]
pub struct ApprovalRequest {
    /// Connector that originated the action (e.g. `connector-github`).
    pub connector_name: String,
    /// Discriminant string of the [`springtale_core::rule::action::Action`]
    /// variant (e.g. `"DeleteFile"`). Sentinel-side we don't carry the
    /// full action payload — the gate is for user prompting, not for
    /// re-validating the action's contents.
    pub action_type: String,
    /// Short human-readable rationale rendered to the user
    /// ("`connector-github` is about to delete a branch"). The
    /// caller composes this; we don't try to synthesize it here.
    pub rationale: String,
}

/// Trait the sentinel calls when a destructive action wants to run.
///
/// `request_approval` returns `true` to proceed, `false` to deny.
/// Implementations are responsible for their own timeouts — the
/// sentinel waits on the future indefinitely so a buggy gate that
/// never completes will hang. [`ChannelApprovalGate`] honours a
/// configurable timeout to avoid this.
#[async_trait]
pub trait ApprovalGate: Send + Sync {
    async fn request_approval(&self, request: ApprovalRequest) -> bool;
}

/// Production-safe default: destructive actions never run unless
/// a real gate is wired. Matches the "default-safe" project rule.
pub struct DefaultDenyApprovalGate;

#[async_trait]
impl ApprovalGate for DefaultDenyApprovalGate {
    async fn request_approval(&self, request: ApprovalRequest) -> bool {
        tracing::warn!(
            connector = %request.connector_name,
            action = %request.action_type,
            "destructive action auto-denied (DefaultDenyApprovalGate)"
        );
        false
    }
}

/// Test-only convenience: every destructive action proceeds. Never
/// wire this in production paths.
pub struct AutoAllowApprovalGate;

#[async_trait]
impl ApprovalGate for AutoAllowApprovalGate {
    async fn request_approval(&self, _request: ApprovalRequest) -> bool {
        true
    }
}

/// Channel-driven gate for runtime/desktop integration.
///
/// Each `request_approval` call:
/// 1. Allocates a [`tokio::sync::oneshot`] for the decision.
/// 2. Sends `(request, response_tx)` on the broadcast sender.
///    A consumer on the other end (the desktop AppState's
///    approval-dispatcher task) translates this into a frontend
///    prompt via a Tauri event, then resolves the oneshot when
///    the user clicks Approve or Deny.
/// 3. Awaits the oneshot with a timeout; falls back to deny.
///
/// The timeout matters in coercive settings: a survivor who steps
/// away mid-prompt shouldn't have a destructive action sit pending
/// indefinitely. Default 60s; configurable per gate.
pub struct ChannelApprovalGate {
    tx: tokio::sync::mpsc::UnboundedSender<PendingApproval>,
    timeout: Duration,
}

/// What flows across the channel: the request, plus a oneshot the
/// consumer uses to send the verdict back.
pub struct PendingApproval {
    pub request: ApprovalRequest,
    pub respond: tokio::sync::oneshot::Sender<bool>,
}

impl ChannelApprovalGate {
    /// Construct a gate plus its receiver. The caller owns the
    /// receiver and is responsible for dispatching each pending
    /// approval — typically by emitting a Tauri event and storing
    /// the `respond` channel keyed by an id the frontend can refer
    /// back to when the user clicks the dialog button.
    pub fn new(timeout: Duration) -> (Self, tokio::sync::mpsc::UnboundedReceiver<PendingApproval>) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        (Self { tx, timeout }, rx)
    }
}

#[async_trait]
impl ApprovalGate for ChannelApprovalGate {
    async fn request_approval(&self, request: ApprovalRequest) -> bool {
        let (respond, await_resp) = tokio::sync::oneshot::channel();
        if let Err(e) = self.tx.send(PendingApproval {
            request: request.clone(),
            respond,
        }) {
            tracing::warn!(
                error = %e,
                connector = %request.connector_name,
                "approval-gate receiver dropped — denying destructive action"
            );
            return false;
        }
        match tokio::time::timeout(self.timeout, await_resp).await {
            Ok(Ok(decision)) => decision,
            Ok(Err(_)) => {
                tracing::warn!(
                    connector = %request.connector_name,
                    "approval-gate responder dropped without answer — denying"
                );
                false
            }
            Err(_) => {
                tracing::warn!(
                    connector = %request.connector_name,
                    timeout_ms = self.timeout.as_millis() as u64,
                    "approval-gate timed out — denying destructive action"
                );
                false
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn req() -> ApprovalRequest {
        ApprovalRequest {
            connector_name: "test".into(),
            action_type: "DeleteFile".into(),
            rationale: "test".into(),
        }
    }

    #[tokio::test]
    async fn default_deny_denies() {
        assert!(!DefaultDenyApprovalGate.request_approval(req()).await);
    }

    #[tokio::test]
    async fn auto_allow_allows() {
        assert!(AutoAllowApprovalGate.request_approval(req()).await);
    }

    #[tokio::test]
    async fn channel_gate_propagates_user_decision() {
        let (gate, mut rx) = ChannelApprovalGate::new(Duration::from_secs(5));
        let handle = tokio::spawn(async move { gate.request_approval(req()).await });
        let pending = rx.recv().await.unwrap();
        pending.respond.send(true).unwrap();
        assert!(handle.await.unwrap());
    }

    #[tokio::test]
    async fn channel_gate_times_out_to_deny() {
        let (gate, _rx) = ChannelApprovalGate::new(Duration::from_millis(30));
        // Don't respond on rx — gate should hit timeout and deny.
        assert!(!gate.request_approval(req()).await);
    }

    #[tokio::test]
    async fn channel_gate_denies_when_responder_drops() {
        let (gate, mut rx) = ChannelApprovalGate::new(Duration::from_secs(5));
        let handle = tokio::spawn(async move { gate.request_approval(req()).await });
        let pending = rx.recv().await.unwrap();
        drop(pending.respond);
        assert!(!handle.await.unwrap());
    }
}
