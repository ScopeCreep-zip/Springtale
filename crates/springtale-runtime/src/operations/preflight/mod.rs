//! W1.D — Preflight checklist before Deploy.
//!
//! Validates everything a recipe needs to actually function before
//! the user clicks Deploy. Per `feedback_preflight_zero_to_live`, the
//! worst UX is a bot that "deploys" but silently does nothing — this
//! module makes those failure modes visible *in the form* with the
//! exact fix needed.
//!
//! Architecture: every classification (Blocking / Warning / Verified /
//! Pending) lives in Rust. The frontend renders the report and
//! decides whether to enable the Deploy button based on the report's
//! `deployable` field — never re-deriving it.
//!
//! Submodules:
//!   - `types.rs` — `PreflightReport`, `PreflightItem`, `PreflightStatus`,
//!     `PreflightFix` (typed fix-hint for the UI).
//!   - `checks.rs` — individual pluggable checks (required inputs,
//!     format validation, connector loaded, AI config).
//!   - `engine.rs` — orchestrator; returns the aggregated report.

pub mod checks;
pub mod engine;
pub mod types;

pub use engine::preflight_recipe;
pub use types::{PreflightFix, PreflightItem, PreflightReport, PreflightStatus};
