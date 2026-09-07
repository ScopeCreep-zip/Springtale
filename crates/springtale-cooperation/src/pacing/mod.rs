//! Pacing — Booth's L4D Director loop applied to a bot formation.
//!
//! Booth, *The AI Systems of Left 4 Dead*, GDC 2009, slides 79–92:
//! intensity is stress. It rises when the survivors are harmed and decays
//! over time — never while they are actively engaged. When it crosses the
//! peak threshold the Director backs off for a while, then builds up
//! again. "Algorithm adjusts pacing, not difficulty. Amplitude
//! (difficulty) is not changed, frequency (pacing) is."
//!
//! Mapped to a formation: harm is failures, interference, sentinel
//! throttles and approval denials. Backing off is the tick divider and a
//! `Relax` phase in which the formation senses but does not act. There
//! are no per-phase action quotas — per-connector rate limits stay in the
//! sentinel.
//!
//! Every number in that loop is per-formation configuration, not a
//! constant (plan 1.5) — see `config.rs`.
//!
//! File split:
//! - `types.rs` — phase + transition enums
//! - `config.rs` — `[cooperation.pacing]` numbers + their defaults
//! - `manager.rs` — stress sample, intensity, transitions, divider, gate

pub mod config;
pub mod manager;
pub mod types;

pub use config::{
    DECAY_PER_SEC, PEAK_THRESHOLD, PacingConfig, RELAX_SECS, SUSTAIN_SECS, W_DENIAL, W_FAILURE,
    W_INTERFERENCE, W_THROTTLE,
};
pub use manager::{PacingManager, StressSample};
pub use types::{PacingPhase, PacingTransition};
