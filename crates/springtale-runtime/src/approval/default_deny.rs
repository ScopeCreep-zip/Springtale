//! Deny-after-timeout `ApprovalGate` impl.
//!
//! Holds pending requests in a `DashMap` keyed by `ApprovalRequestId`.
//! Each entry stores a `tokio::sync::oneshot::Sender` the management
//! API resolves into; the requestor `await`s the receiver with a
//! `tokio::time::timeout` fence. Timeout → the gate writes
//! `TimedOut` into the entry's slot, returns to the requestor, and
//! removes the row.
//!
//! Default timeout is 60s ([`super::DEFAULT_APPROVAL_TIMEOUT`]) so a
//! distracted maintainer's pending notification has a real chance of
//! resolving before deny-fallback kicks in.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use dashmap::DashMap;
use tokio::sync::{Mutex, oneshot};

use super::gate::{
    ApprovalDecision, ApprovalError, ApprovalGate, ApprovalRequest, ApprovalRequestId,
    DEFAULT_APPROVAL_TIMEOUT,
};

/// Production default gate: blocks until a `POST /approvals/:id`
/// resolves the request, falls back to **deny** after `timeout`.
pub struct DefaultDenyApprovalGate {
    pending: Arc<DashMap<ApprovalRequestId, PendingEntry>>,
    timeout: Duration,
}

struct PendingEntry {
    request: ApprovalRequest,
    /// `Mutex<Option<...>>` rather than `oneshot::Sender` directly so
    /// the management API path can take ownership of the sender
    /// inside an async lock without needing `DashMap::remove` (which
    /// would race with the requestor's `await`).
    responder: Mutex<Option<oneshot::Sender<ApprovalDecision>>>,
}

impl DefaultDenyApprovalGate {
    /// Build a gate with the workspace default 60s timeout.
    pub fn new() -> Self {
        Self::with_timeout(DEFAULT_APPROVAL_TIMEOUT)
    }

    /// Build a gate with a custom timeout — used by tests that need
    /// a short fence to assert the deny-fallback path.
    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            pending: Arc::new(DashMap::new()),
            timeout,
        }
    }
}

impl Default for DefaultDenyApprovalGate {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ApprovalGate for DefaultDenyApprovalGate {
    async fn request(&self, request: ApprovalRequest) -> Result<ApprovalDecision, ApprovalError> {
        let id = request.id;
        let (tx, rx) = oneshot::channel();
        self.pending.insert(
            id,
            PendingEntry {
                request,
                responder: Mutex::new(Some(tx)),
            },
        );

        let outcome = tokio::time::timeout(self.timeout, rx).await;
        // Always clean the row out — every exit path removes the
        // request from the pending map.
        self.pending.remove(&id);

        match outcome {
            // User landed a decision.
            Ok(Ok(decision)) => Ok(decision),
            // Sender side dropped without sending (shouldn't happen
            // under our impl but the channel error is a real
            // contract).
            Ok(Err(_)) => Ok(ApprovalDecision::TimedOut {
                timed_out_at: Utc::now(),
            }),
            // Timeout fired before any resolve arrived.
            Err(_) => Ok(ApprovalDecision::TimedOut {
                timed_out_at: Utc::now(),
            }),
        }
    }

    async fn resolve(
        &self,
        id: ApprovalRequestId,
        decision: ApprovalDecision,
    ) -> Result<(), ApprovalError> {
        let entry = self.pending.get(&id).ok_or(ApprovalError::Unknown(id))?;
        let mut slot = entry.responder.lock().await;
        let sender = slot.take().ok_or(ApprovalError::DuplicateResolve(id))?;
        // Drop the lock before sending so the requestor's wake-up
        // can't trip on the lock guard.
        drop(slot);
        // Releasing the DashMap ref so request()'s remove can proceed.
        drop(entry);
        // Ignore SendError — the requestor side might have already
        // timed out; in that case we accept the resolve as a no-op,
        // not an error (avoids races where the user clicks Approve
        // moments before the 60s fence fires).
        let _ = sender.send(decision);
        Ok(())
    }

    async fn pending(&self) -> Vec<ApprovalRequest> {
        self.pending
            .iter()
            .map(|entry| entry.value().request.clone())
            .collect()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use springtale_connector::manifest::types::Capability;

    fn req() -> ApprovalRequest {
        ApprovalRequest {
            id: ApprovalRequestId::new(),
            connector_name: "connector-shell".into(),
            capability: crate::approval::GatedCapability::Manifest(Capability::ShellExec),
            agent_id: None,
            summary: "test request".into(),
            requested_at: Utc::now(),
            origin: None,
        }
    }

    #[tokio::test]
    async fn approves_when_user_approves_in_time() {
        let gate = Arc::new(DefaultDenyApprovalGate::with_timeout(Duration::from_secs(
            5,
        )));
        let r = req();
        let id = r.id;
        let g2 = Arc::clone(&gate);
        let handle = tokio::spawn(async move { g2.request(r).await });

        // Give the requestor a tick to land in pending.
        tokio::time::sleep(Duration::from_millis(20)).await;
        gate.resolve(
            id,
            ApprovalDecision::Approved {
                approver: "maintainer".into(),
                approved_at: Utc::now(),
            },
        )
        .await
        .unwrap();

        let decision = handle.await.unwrap().unwrap();
        assert!(decision.is_approved());
    }

    #[tokio::test]
    async fn denies_when_user_denies() {
        let gate = Arc::new(DefaultDenyApprovalGate::with_timeout(Duration::from_secs(
            5,
        )));
        let r = req();
        let id = r.id;
        let g2 = Arc::clone(&gate);
        let handle = tokio::spawn(async move { g2.request(r).await });

        tokio::time::sleep(Duration::from_millis(20)).await;
        gate.resolve(
            id,
            ApprovalDecision::Denied {
                reason: "no thanks".into(),
                denied_at: Utc::now(),
            },
        )
        .await
        .unwrap();

        let decision = handle.await.unwrap().unwrap();
        assert!(!decision.is_approved());
        match decision {
            ApprovalDecision::Denied { reason, .. } => assert_eq!(reason, "no thanks"),
            other => panic!("expected Denied, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn times_out_to_deny_fallback() {
        // 50 ms timeout — no user response — must surface TimedOut.
        let gate = DefaultDenyApprovalGate::with_timeout(Duration::from_millis(50));
        let decision = gate.request(req()).await.unwrap();
        assert!(
            matches!(decision, ApprovalDecision::TimedOut { .. }),
            "expected TimedOut, got {decision:?}"
        );
        // And the pending map is clean.
        assert!(gate.pending().await.is_empty());
    }

    #[tokio::test]
    async fn duplicate_resolve_fails() {
        let gate = Arc::new(DefaultDenyApprovalGate::with_timeout(Duration::from_secs(
            5,
        )));
        let r = req();
        let id = r.id;
        let g2 = Arc::clone(&gate);
        let handle = tokio::spawn(async move { g2.request(r).await });
        tokio::time::sleep(Duration::from_millis(20)).await;

        // First resolve wins.
        gate.resolve(
            id,
            ApprovalDecision::Approved {
                approver: "maintainer".into(),
                approved_at: Utc::now(),
            },
        )
        .await
        .unwrap();

        // Second resolve is rejected.
        let err = gate
            .resolve(
                id,
                ApprovalDecision::Denied {
                    reason: "too late".into(),
                    denied_at: Utc::now(),
                },
            )
            .await;
        assert!(
            matches!(err, Err(ApprovalError::Unknown(_)))
                || matches!(err, Err(ApprovalError::DuplicateResolve(_)))
        );

        let _ = handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn resolve_unknown_id_errors() {
        let gate = DefaultDenyApprovalGate::new();
        let err = gate
            .resolve(
                ApprovalRequestId::new(),
                ApprovalDecision::Approved {
                    approver: "maintainer".into(),
                    approved_at: Utc::now(),
                },
            )
            .await;
        assert!(matches!(err, Err(ApprovalError::Unknown(_))));
    }

    #[tokio::test]
    async fn pending_snapshot_lists_outstanding() {
        let gate = Arc::new(DefaultDenyApprovalGate::with_timeout(Duration::from_secs(
            5,
        )));
        let r = req();
        let g2 = Arc::clone(&gate);
        tokio::spawn(async move {
            let _ = g2.request(r).await;
        });
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert_eq!(gate.pending().await.len(), 1);
    }
}
