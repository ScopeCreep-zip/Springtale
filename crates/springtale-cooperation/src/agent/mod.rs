//! Agent-side tick composition.
//!
//! Hosts `loop_::tick` — the canonical ordering of the per-step pipeline
//! (`step/sense.rs` → `step/inbox.rs` → `step/react.rs` (awareness only)
//! → `step/scan.rs`). CFP responses (`step/respond_cfp.rs`) are wired in
//! by B2 (Contract Net). Each step is in its own file so it can be
//! unit-tested against mocked deps; the loop composition is in `loop_.rs`.

pub mod context;
pub mod loop_;
pub mod result;
pub mod step;

pub use context::AgentContext;
pub use loop_::AgentLoop;
pub use result::AgentTickResult;
