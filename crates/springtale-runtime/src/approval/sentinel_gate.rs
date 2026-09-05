//! Sentinel → chat gate adapter (plan 6.7).
//!
//! The sentinel prompts through `springtale_sentinel::ApprovalGate` when
//! an action is destructive; the runtime's [`ChatApprovalGate`] prompts
//! through chat cards + the dashboard (`GET/POST /approvals`). Before
//! this adapter the two never met: a shell that supplied no UI gate
//! (springtaled, CLI) fell back to sentinel-side default-deny and nobody
//! was asked. `SentinelChatGate` maps the sentinel request onto the chat
//! gate, carrying the per-request chat origin through so the card lands
//! where the message came from.

use std::sync::Arc;

use async_trait::async_trait;

use super::chat_gate::ChatApprovalGate;
use super::{ApprovalDecision, ApprovalGate, ApprovalRequest, ApprovalRequestId, GatedCapability};

/// Thin wrapper: one shared [`ChatApprovalGate`], sentinel-shaped API.
pub struct SentinelChatGate {
    inner: Arc<ChatApprovalGate>,
}

impl SentinelChatGate {
    pub fn new(inner: Arc<ChatApprovalGate>) -> Self {
        Self { inner }
    }

    /// Map a sentinel request onto the runtime request the chat gate
    /// persists and announces. `agent_id` is `None`: the sentinel does
    /// not carry it; the audit row that flanks the request does.
    pub fn map_request(req: &springtale_sentinel::ApprovalRequest) -> ApprovalRequest {
        ApprovalRequest {
            id: ApprovalRequestId::new(),
            connector_name: req.connector_name.clone(),
            capability: GatedCapability::DestructiveAction {
                action_type: req.action_type.clone(),
            },
            agent_id: None,
            summary: req.rationale.clone(),
            requested_at: chrono::Utc::now(),
            origin: req.origin.clone(),
            expires_at: None,
        }
    }
}

#[async_trait]
impl springtale_sentinel::ApprovalGate for SentinelChatGate {
    async fn request_approval(&self, request: springtale_sentinel::ApprovalRequest) -> bool {
        matches!(
            self.inner.request(Self::map_request(&request)).await,
            Ok(ApprovalDecision::Approved { .. })
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use springtale_core::policy::ChatOrigin;

    #[test]
    fn test_map_request_with_origin_carries_origin() {
        let origin = ChatOrigin {
            connector: "connector-telegram".to_owned(),
            channel_id: "chat-42".to_owned(),
        };
        let mapped = SentinelChatGate::map_request(&springtale_sentinel::ApprovalRequest {
            connector_name: "connector-github".to_owned(),
            action_type: "RunConnector".to_owned(),
            rationale: "about to delete a branch".to_owned(),
            origin: Some(origin.clone()),
        });
        assert_eq!(mapped.origin, Some(origin));
        assert_eq!(mapped.connector_name, "connector-github");
        assert_eq!(mapped.summary, "about to delete a branch");
        assert!(matches!(
            mapped.capability,
            GatedCapability::DestructiveAction { ref action_type } if action_type == "RunConnector"
        ));
    }
}
