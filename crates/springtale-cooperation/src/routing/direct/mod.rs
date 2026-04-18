//! L3 direct handoff — per-agent assigned-task inbox.
//!
//! When an agent posts a SubTask with `assigned_to: Some(target)`, the
//! target's inbox records the task id. The agent loop checks its inbox before
//! scanning the general pool.

pub mod assignment;
pub mod inbox;

pub use inbox::DirectInbox;
