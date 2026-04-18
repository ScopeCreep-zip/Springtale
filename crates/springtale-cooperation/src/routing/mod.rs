//! L1 (routine pull+scan) and L3 (direct handoff) task routing.
//!
//! Family 1 (RimWorld JobGiver_Work) semantics — every agent scans a global
//! task pool, filters by its own capabilities, claims one — layered over
//! Family 3 (Celery/Temporal queue-per-capability) as a capability-indexed
//! lookup that keeps the scan O(caps·priority-depth) instead of O(all-tasks).
//!
//! See plan Phase K §L1 + §L3.

pub mod claim;
pub mod direct;
pub mod index;
pub mod scan;
pub mod trait_;
pub mod types;

pub use trait_::TaskRouter;
pub use types::{PriorityTask, RoutingError, TaskClaim, TaskId};
