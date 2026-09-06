//! Shared rule-action dispatcher — executes matched rule actions.
//!
//! Both springtaled (daemon) and springtale-bot route action dispatch
//! through this module. Per ARCHITECTURE.md §6.10, this is the single
//! enforcement point that calls `sentinel.evaluate()` before every
//! action.
//!
//! ## Phase 0 rework (chain context + real I/O)
//!
//! Pre-Phase-0 this module returned `Result<String, String>` and
//! discarded step outputs between iterations. `Action::AiComplete`
//! was a stub returning `"ai: noop"`. `Action::RunConnector` captured
//! only the connector's plain-text `message` and dropped its
//! structured `output: Value`. The net effect: ~12 shipped builtin
//! recipes referenced `${last_ai_output}` / `${last_connector_output}`
//! in their TOML, but the dispatcher had no path that resolved those
//! placeholders — users received literal `${last_ai_output}` strings
//! in their messaging channels.
//!
//! The new shape closes all four gaps:
//!
//! 1. Return type is `Result<ChainContext, ChainError>` —
//!    `ChainContext` carries every step's typed `output`, the
//!    `last_*_output` aliases, and the trigger payload. Callers read
//!    it to surface results or persist to the executions log.
//! 2. Before each step runs, action parameters are template-resolved
//!    against the chain via [`resolve_chain_value`] — `${trigger.x}`,
//!    `${last_ai_output}`, `${stepN.field}`, `${step.NAME.field}` all
//!    bind to live values.
//! 3. `RunConnector` captures the connector's `ActionResult.output`
//!    (the structured JSON) into `StepOutput.output`, not just the
//!    human message.
//! 4. `AiComplete` calls the real adapter via
//!    [`CapabilityBridge::ai_adapter_for`] — falls back to
//!    `NoopAdapter` (clean error, not silent stubbing) when no
//!    adapter is wired.
//!
//! Cooperation alignment: every dispatch carries an
//! [`ExecutionContext`] from `springtale-cooperation::execution`, so
//! the runtime knows which agent in which formation at which momentum
//! tier is firing. The bridge consults the tier for capability
//! routing (per-tier WASM `InstancePre` selection, §16). The sentinel
//! consults the tier for rate-budget scaling: Cold = 1/30s, Warming =
//! 12/min, Hot = 60/min, Fever = 600/min — the Phase 0.5 mapping in
//! [`crate::cooperation::momentum_to_throttle_tier`].

pub mod chain;
pub mod connector;
pub mod entry;
pub mod extract;
pub mod step;

pub use entry::{dispatch_action, dispatch_actions};
