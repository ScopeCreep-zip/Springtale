//! Agent lifecycle supervision — Erlang OTP + Kubernetes probes + FAILURE.md.
//!
//! Per COOPERATION.md §15: formations must detect and recover from member
//! failures. This module provides the detection (liveness probes), the
//! classification (FAILURE.md categories), and the policy (Erlang restart
//! intensity).
//!
//! The event loop calls `supervisor.check_member()` per member per tick.
//! The supervisor returns a `SupervisionAction` that the event loop
//! dispatches — transform role, retry with rally, trigger CBBA replan,
//! or escalate to L6 intervention.

pub mod failure;
pub mod liveness;
pub mod restart;
pub mod supervisor;

pub use failure::FailureCategory;
pub use liveness::Liveness;
pub use restart::{RestartPolicy, RestartStrategy};
pub use supervisor::{FormationSupervisor, SupervisionAction};
