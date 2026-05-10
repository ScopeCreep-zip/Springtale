//! Cooperation events — typed taxonomy + envelope for the
//! `CooperationEventEnvelope` broadcast stream (Phase H).
//!
//! The bus itself is owned by `springtale-runtime::RuntimeState` (next to
//! `canvas_tx` per H2); this module owns the event type definitions only.
//! Subscribers read via the SSE endpoint `/cooperation/events` (web,
//! Phase H3) or the Tauri `subscribe_cooperation` IPC channel
//! (desktop, Phase H4).

pub mod types;

pub use types::{
    CooperationEvent, CooperationEventEnvelope, InterferenceKind, InterventionKind,
    ReplanOutcomeSummary, VoteOutcome,
};
