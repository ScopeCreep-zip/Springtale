//! Blocking-approval gate for capabilities that cannot be
//! auto-granted under any `CapabilityPolicy` — currently
//! `Capability::ShellExec`, which is the OpenClaw CVE-2026-25253
//! 1-click-RCE class Springtale exists to defeat.
//!
//! The capability layer (`springtale-connector::capability::grant`)
//! routes ShellExec into `pending_approval` regardless of policy
//! (see Phase-7 audit Finding A in
//! `~/.claude/plans/mighty-honking-pinwheel.md`). The dispatch layer
//! (`springtale-runtime::dispatch`) awaits this gate before forwarding
//! to `host.execute_checked()`. The default impl
//! [`DefaultDenyApprovalGate`] blocks until a user response arrives
//! via the management API (`POST /approvals/:id`) with HMAC bearer
//! auth, then falls back to **deny** after a configurable timeout
//! (default 60s) so a dropped connection never silently grants.
//!
//! Every decision lands in the `springtale-sentinel` audit trail as
//! an `ApprovalRequested` / `ApprovalResolved` row so forensic review
//! can reconstruct the decision history.

mod chat_gate;
mod default_deny;
mod gate;
mod sentinel_gate;

pub use chat_gate::{CHAT_APPROVAL_TIMEOUT, ChatApprovalGate};
pub use default_deny::DefaultDenyApprovalGate;
pub use gate::{
    ApprovalDecision, ApprovalError, ApprovalGate, ApprovalRequest, ApprovalRequestId,
    DEFAULT_APPROVAL_TIMEOUT, GatedCapability,
};
pub use sentinel_gate::SentinelChatGate;
