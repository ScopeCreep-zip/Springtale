//! Strategic (colony) layer of the AI command hierarchy.
//!
//! The [`commander`] orchestrates ACROSS formations — the RTS "commander" above
//! the per-formation squad orchestrators. It runs every [`COLONY_INTERVAL`]
//! cadence ticks from the bot event loop, AFTER the per-formation tick, so it
//! never holds the formation lock while awaiting an LLM call.
//!
//! Like every layer of the hierarchy it is AI-optional: with no `ai:colony`
//! adapter it runs a deterministic cross-formation policy (de-escalate
//! panicking formations); with an adapter it asks the colony AI to propose
//! per-formation intent moves, validated against the live colony before
//! applying. Guarded formations are never auto-touched.

pub mod commander;

/// Cadence ticks between colony reviews. At the 30 Hz cadence default that is
/// ~1 review/second — strategic cadence, not per-tick.
pub const COLONY_INTERVAL: u64 = 30;

/// Cascade-hit streak (at Cold tier) above which the deterministic policy
/// de-escalates a formation to Stabilize. WH3-style: sustained panic with no
/// momentum ⇒ pull back rather than thrash.
pub const DEESCALATE_STREAK: u32 = 3;
