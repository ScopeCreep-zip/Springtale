//! Action dispatcher — thin delegation to springtale_runtime::dispatch.
//!
//! All action execution logic lives in the shared runtime crate so
//! both springtaled and springtale-bot use the same implementation.

pub use springtale_runtime::dispatch::dispatch_action;
