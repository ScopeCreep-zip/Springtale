//! Per-agent task executor, split at the beat boundary (plan 1.8):
//!
//! - [`prepare`] — everything that needs the formation: sacrifice
//!   short-circuit, active-task continuation, autonomy gate, pacing gate,
//!   blackboard claim, consensus gate. Returns either a settled outcome
//!   or a [`DispatchJob`] that owns every handle the dispatch needs.
//! - [`dispatch_one`] — the connector call. No `&mut Formation`; spawnable,
//!   so members act together inside `tick.window` and a slow member is
//!   carried to a later beat instead of stalling the rest.
//! - [`post`] — the member's own write-back: active-task state, result
//!   row, W3 push handoff, audit write, stigmergy deposit, attention
//!   sample, `TickReport`. Arbitrates nothing; `tick_processor` runs over
//!   the beat's write log afterwards (§13).
//!
//! Autonomy (AoE stances): Observe reports the step descriptor only;
//! Suggest logs without claiming; Approve and Autonomous both dispatch —
//! the sentinel gate inside `dispatch_action` decides whether a human is
//! asked (0.1 / 0.3), and a member waiting on a human is just a member
//! whose dispatch has not finished this beat.

mod dispatch;
mod post;
mod prepare;
#[cfg(test)]
pub(crate) mod test_support;
#[cfg(test)]
mod tests;

pub use crate::cooperation::dispatch_outcome::{Dispatched, ExecuteOutcome};
pub use dispatch::{DispatchJob, dispatch_one};
pub use post::post;
pub use prepare::{ExecuteCtx, Prepared, prepare};
