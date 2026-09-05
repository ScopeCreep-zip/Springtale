//! Store-backed `ApprovalGate` with chat delivery (W2).
//!
//! The 2026 enterprise HITL interrupt pattern, native: a pending approval is
//! a **durable store row** (survives restart) plus an in-process fast path
//! (oneshot waiter). A broadcast channel announces each new request so the
//! bot-side notifier can deliver a 3-button card to the owner's chat channel
//! (Telegram inline keyboard; numbered-reply fallback elsewhere). Decisions
//! arrive through the same [`ApprovalGate::resolve`] contract the management
//! API already uses — `POST /approvals/:id` keeps working unchanged.
//!
//! Deny-by-default is preserved end-to-end: timeout ⇒ `TimedOut`; rows past
//! expiry are stamped denied by the boot sweep
//! (`StorageBackend::expire_pending_approvals`); a dropped phone never
//! silently grants.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use dashmap::DashMap;
use tokio::sync::{Mutex, broadcast, oneshot};

use springtale_store::{PendingApprovalRow, StorageBackend};

use super::gate::{
    ApprovalDecision, ApprovalError, ApprovalGate, ApprovalRequest, ApprovalRequestId,
};

/// Chat approvals wait much longer than the in-API gate's 60s — the owner
/// may be away from their phone. Deny still lands at expiry.
pub const CHAT_APPROVAL_TIMEOUT: Duration = Duration::from_secs(15 * 60);

/// Store-backed gate with chat-card delivery.
pub struct ChatApprovalGate {
    store: Arc<dyn StorageBackend>,
    timeout: Duration,
    /// New-request announcements for the bot-side notifier task.
    notify_tx: broadcast::Sender<ApprovalRequest>,
    /// In-process fast path: requestors awaiting a live decision.
    waiters: DashMap<ApprovalRequestId, Mutex<Option<oneshot::Sender<ApprovalDecision>>>>,
}

impl ChatApprovalGate {
    pub fn new(store: Arc<dyn StorageBackend>) -> Self {
        Self::with_timeout(store, CHAT_APPROVAL_TIMEOUT)
    }

    pub fn with_timeout(store: Arc<dyn StorageBackend>, timeout: Duration) -> Self {
        let (notify_tx, _) = broadcast::channel(64);
        Self {
            store,
            timeout,
            notify_tx,
            waiters: DashMap::new(),
        }
    }

    /// Subscribe to new-request announcements (the bot notifier task).
    pub fn subscribe(&self) -> broadcast::Receiver<ApprovalRequest> {
        self.notify_tx.subscribe()
    }

    fn row_from(&self, req: &ApprovalRequest) -> PendingApprovalRow {
        PendingApprovalRow {
            id: req.id.to_string(),
            connector_name: req.connector_name.clone(),
            capability_json: serde_json::to_string(&req.capability)
                .unwrap_or_else(|_| "\"unknown\"".to_owned()),
            agent_id: req.agent_id.map(|a| a.to_string()),
            summary: req.summary.clone(),
            requested_at: req.requested_at.timestamp_millis(),
            expires_at: (req.requested_at
                + chrono::Duration::from_std(self.timeout)
                    .unwrap_or_else(|_| chrono::Duration::seconds(900)))
            .timestamp_millis(),
            decision_json: None,
        }
    }
}

#[async_trait]
impl ApprovalGate for ChatApprovalGate {
    async fn request(&self, request: ApprovalRequest) -> Result<ApprovalDecision, ApprovalError> {
        let id = request.id;

        // 1. Durable first — the row IS the pause (restart-safe).
        if let Err(e) = self
            .store
            .insert_pending_approval(self.row_from(&request))
            .await
        {
            tracing::error!(approval = %id, error = %e, "pending approval persist failed");
            return Err(ApprovalError::Shutdown);
        }

        // 2. Register the in-process waiter, then announce to the notifier.
        let (tx, rx) = oneshot::channel();
        self.waiters.insert(id, Mutex::new(Some(tx)));
        let _ = self.notify_tx.send(request);

        // 3. Block until decision or timeout (deny-by-default).
        let decision = match tokio::time::timeout(self.timeout, rx).await {
            Ok(Ok(decision)) => decision,
            // Channel dropped or timeout — stamp timed-out durably.
            _ => {
                let timed_out = ApprovalDecision::TimedOut {
                    timed_out_at: Utc::now(),
                };
                let json = serde_json::to_string(&timed_out)
                    .unwrap_or_else(|_| "{\"kind\":\"timed_out\"}".to_owned());
                let _ = self
                    .store
                    .resolve_pending_approval(&id.to_string(), &json)
                    .await;
                timed_out
            }
        };
        self.waiters.remove(&id);
        Ok(decision)
    }

    async fn resolve(
        &self,
        id: ApprovalRequestId,
        decision: ApprovalDecision,
    ) -> Result<(), ApprovalError> {
        // Durable decision first; `false` ⇒ missing or already decided.
        let json = serde_json::to_string(&decision).map_err(|_| ApprovalError::Unknown(id))?;
        let landed = self
            .store
            .resolve_pending_approval(&id.to_string(), &json)
            .await
            .map_err(|_| ApprovalError::Shutdown)?;
        if !landed {
            // Distinguish duplicate vs unknown for the API's 409/404 split.
            let exists = self
                .store
                .get_pending_approval(&id.to_string())
                .await
                .ok()
                .flatten()
                .is_some();
            return Err(if exists {
                ApprovalError::DuplicateResolve(id)
            } else {
                ApprovalError::Unknown(id)
            });
        }

        // Fast path: wake a live waiter when the loop is still in-process.
        // After a restart there is no waiter — the checkpoint resumer picks
        // the decision up from the store instead.
        if let Some(entry) = self.waiters.get(&id)
            && let Some(tx) = entry.lock().await.take()
        {
            let _ = tx.send(decision);
        }
        Ok(())
    }

    async fn pending(&self) -> Vec<ApprovalRequest> {
        let rows = self
            .store
            .list_pending_approvals(Utc::now().timestamp_millis())
            .await
            .unwrap_or_default();
        rows.into_iter()
            .filter_map(|r| {
                Some(ApprovalRequest {
                    id: ApprovalRequestId(uuid::Uuid::parse_str(&r.id).ok()?),
                    connector_name: r.connector_name,
                    capability: serde_json::from_str(&r.capability_json).ok()?,
                    agent_id: r
                        .agent_id
                        .and_then(|a| uuid::Uuid::parse_str(&a).ok())
                        .map(springtale_cooperation::cadence::AgentId),
                    summary: r.summary,
                    requested_at: chrono::DateTime::from_timestamp_millis(r.requested_at)?,
                    origin: None,
                    expires_at: chrono::DateTime::from_timestamp_millis(r.expires_at),
                })
            })
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
            summary: "exec: echo hi".into(),
            requested_at: Utc::now(),
            origin: None,
            expires_at: None,
        }
    }

    fn store() -> Arc<dyn StorageBackend> {
        Arc::new(springtale_store::SqliteBackend::open_in_memory().unwrap())
    }

    #[tokio::test]
    async fn approve_over_resolve_unblocks_request() {
        let gate = Arc::new(ChatApprovalGate::with_timeout(
            store(),
            Duration::from_secs(5),
        ));
        let mut announced = gate.subscribe();
        let r = req();
        let id = r.id;

        let g2 = Arc::clone(&gate);
        let waiter = tokio::spawn(async move { g2.request(r).await });

        // Notifier sees the announcement, then "the user taps approve".
        let seen = announced.recv().await.unwrap();
        assert_eq!(seen.id, id);
        gate.resolve(
            id,
            ApprovalDecision::Approved {
                approver: "owner".into(),
                approved_at: Utc::now(),
            },
        )
        .await
        .unwrap();

        let decision = waiter.await.unwrap().unwrap();
        assert!(decision.is_approved());
        // Decision landed durably too.
        let row = gate
            .store
            .get_pending_approval(&id.to_string())
            .await
            .unwrap()
            .unwrap();
        assert!(row.decision_json.unwrap().contains("approved"));
    }

    #[tokio::test]
    async fn timeout_denies_and_persists() {
        let gate = ChatApprovalGate::with_timeout(store(), Duration::from_millis(50));
        let r = req();
        let id = r.id;
        let decision = gate.request(r).await.unwrap();
        assert!(!decision.is_approved());
        let row = gate
            .store
            .get_pending_approval(&id.to_string())
            .await
            .unwrap()
            .unwrap();
        assert!(row.decision_json.unwrap().contains("timed_out"));
    }

    #[tokio::test]
    async fn duplicate_resolve_is_rejected() {
        let gate = ChatApprovalGate::with_timeout(store(), Duration::from_millis(100));
        let r = req();
        let id = r.id;
        let _ = gate.request(r).await; // times out → decided
        let err = gate
            .resolve(
                id,
                ApprovalDecision::Approved {
                    approver: "owner".into(),
                    approved_at: Utc::now(),
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ApprovalError::DuplicateResolve(_)));
    }
}
