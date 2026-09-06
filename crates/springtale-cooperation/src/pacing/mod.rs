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
//! File split:
//! - `types.rs` — phase + transition enums
//! - `manager.rs` — stress sample, intensity, transitions, divider, gate

pub mod manager;
pub mod types;

pub use manager::{DECAY_PER_SEC, PEAK_THRESHOLD, PacingManager, RELAX, SUSTAIN, StressSample};
pub use types::{PacingPhase, PacingTransition};
