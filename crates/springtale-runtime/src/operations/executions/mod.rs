//! Executions log — chain-fire observability (Phase B).
//!
//! Three layers split per `feedback_thin_frontend_modular_backend`:
//!
//! 1. `recorder.rs` — the [`ExecutionRecorder`] trait + production
//!    [`StoreRecorder`] + test-only [`NoopRecorder`]. The
//!    dispatcher calls into the trait; the trait owns the privacy
//!    posture (sizes only, opt-in content via blob refs).
//!
//! 2. `query.rs` — read-side operations the Tauri commands and
//!    web dashboard call via `RuntimeState`: list summaries for an
//!    agent / formation; fetch step rows for one execution; sweep
//!    expired rows.
//!
//! 3. (Phase C) `content.rs` — opt-in content retention via a
//!    separate KV blob store. Out of scope for B.5/B.6.

pub mod drift;
pub mod query;
pub mod recorder;

pub use drift::{
    DriftClass, DriftFilter, DriftReport, LatencyDrift, RateDrift, recipe_drift, rule_drift,
};
pub use query::{
    ExecutionFilterIpc, ExecutionInfo, ExecutionStepInfo, GetStepsError, ListExecutionsError,
    get_execution_steps, get_execution_steps_ipc, list_executions, list_executions_ipc,
    vacuum_executions,
};
pub use recorder::{DEFAULT_RETENTION_MS, ExecutionRecorder, NoopRecorder, StoreRecorder};
