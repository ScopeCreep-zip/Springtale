//! `ApprovalGate` trait + supporting types.
//!
//! The trait is async-object-safe so the bridge can hold an
//! `Arc<dyn ApprovalGate>` without knowing which impl is wired —
//! production uses [`super::DefaultDenyApprovalGate`], tests inject
//! an in-memory permissive impl, and a future native-desktop impl
//! could route to a Tauri dialog instead of the HTTP API.

use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use specta::Type;
use thiserror::Error;
use uuid::Uuid;

use springtale_connector::manifest::types::Capability;
use springtale_cooperation::cadence::AgentId;
use springtale_core::policy::ChatOrigin;

/// Default deny-after timeout for a pending approval. Picked at one
/// minute to give a maintainer time to react via a notification
/// (Tauri tray, mobile push) but short enough that a forgotten
/// approval doesn't pin a running connector forever.
pub const DEFAULT_APPROVAL_TIMEOUT: Duration = Duration::from_secs(60);

/// Opaque per-request identifier. Stable across the gate's lifetime;
/// used as the URL param on `POST /approvals/:id`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub struct ApprovalRequestId(pub Uuid);

impl ApprovalRequestId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ApprovalRequestId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for ApprovalRequestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Inbound payload from the dispatch layer. The gate doesn't need to
/// know the cooperation tier or the firing rule — those live in the
/// audit row that flanks the request, not in the decision contract.
/// What a pending approval is gating. Manifest capabilities come from the
/// `CapabilityBridge` ShellExec gate; destructive actions come from the
/// sentinel via [`super::SentinelChatGate`]. Untagged so rows persisted
/// before plan 6.7 (`"ShellExec"`) still deserialize.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(untagged)]
pub enum GatedCapability {
    Manifest(Capability),
    DestructiveAction { action_type: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ApprovalRequest {
    pub id: ApprovalRequestId,
    /// Connector requesting the dangerous capability. Connector name
    /// matches `ConnectorManifest::name`.
    pub connector_name: String,
    /// The exact capability being requested. Today this is always
    /// `ShellExec`; the contract is open for any future capability
    /// the workspace decides to gate.
    pub capability: GatedCapability,
    /// Caller bot id, when the request originates from a firing rule.
    /// `None` for chat-command / CLI-direct paths where the user is
    /// the active caller already.
    pub agent_id: Option<AgentId>,
    /// Human-readable context the approval UI shows next to the
    /// decision (e.g. the action name + command line summary). Never
    /// the raw stdin / argv — that goes in the audit row.
    pub summary: String,
    /// When this request was created.
    pub requested_at: DateTime<Utc>,
    /// Chat channel the triggering message came from, when one did. The
    /// announcer delivers the approval card there; `None` means the
    /// dashboard is the only surface (deny-on-timeout still applies).
    #[serde(default)]
    pub origin: Option<ChatOrigin>,
}

/// User decision on a pending approval. `Approved` carries who
/// approved + when so the audit row records the chain of custody.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ApprovalDecision {
    Approved {
        approver: String,
        approved_at: DateTime<Utc>,
    },
    Denied {
        reason: String,
        denied_at: DateTime<Utc>,
    },
    TimedOut {
        timed_out_at: DateTime<Utc>,
    },
}

impl ApprovalDecision {
    /// `true` iff the decision authorises execution. Convenience for
    /// the dispatch path which just needs the boolean.
    pub fn is_approved(&self) -> bool {
        matches!(self, Self::Approved { .. })
    }
}

#[derive(Debug, Error)]
pub enum ApprovalError {
    #[error("approval gate is shut down")]
    Shutdown,
    #[error("duplicate resolve for approval {0}")]
    DuplicateResolve(ApprovalRequestId),
    #[error("unknown approval request {0}")]
    Unknown(ApprovalRequestId),
}

/// Async, object-safe blocking-approval contract.
///
/// `request` blocks until a decision arrives (or the timeout fires).
/// `resolve` is called by the management API when the user responds.
/// `pending` lists outstanding requests so the UI can render them.
#[async_trait]
pub trait ApprovalGate: Send + Sync + 'static {
    /// Block until a decision lands for `request`. Returns
    /// `Approved` / `Denied` / `TimedOut`. The gate handles its own
    /// timeout; callers don't wrap this in `tokio::time::timeout`.
    async fn request(&self, request: ApprovalRequest) -> Result<ApprovalDecision, ApprovalError>;

    /// Land a decision for a pending request. Called by the
    /// management API's `POST /approvals/:id` handler. Idempotent:
    /// the second resolve for the same id returns
    /// `DuplicateResolve` so the API can return 409 without crashing.
    async fn resolve(
        &self,
        id: ApprovalRequestId,
        decision: ApprovalDecision,
    ) -> Result<(), ApprovalError>;

    /// Snapshot of currently-pending requests. The admin API uses
    /// this to render the approval queue; UIs poll or subscribe.
    async fn pending(&self) -> Vec<ApprovalRequest>;
}
