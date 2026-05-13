//! Action dispatcher — thin delegation to springtale_runtime::dispatch.
//!
//! All action execution logic lives in the shared runtime crate so
//! both springtaled and springtale-bot use the same implementation.
//! Phase 0: the dispatcher now returns `Result<ChainContext, ChainError>`
//! and requires an `ExecutionContext` at the call site so cooperation
//! scoping (agent / formation / momentum) is explicit.

pub use springtale_runtime::dispatch::{dispatch_action, dispatch_actions};
