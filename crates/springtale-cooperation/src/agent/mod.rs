//! Agent-side tick composition.
//!
//! This module will host `AgentLoop::tick()` — composition-only, ~30 lines —
//! plus per-step files under `step/`. During scaffolding we expose only the
//! shared `AgentContext` so other modules (contract_net, replan) can refer to
//! it without depending on the full step machinery.

pub mod context;
pub mod step;

pub use context::AgentContext;
