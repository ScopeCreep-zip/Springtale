//! L1 Family-3 capability index: `DashMap<capability, BinaryHeap<PriorityTask>>`.
//!
//! Tasks are filed by `target_connector`, ordered within a bucket by priority.
//! Agent scans only the buckets matching its capability list.

pub mod capability;
pub mod priority;

pub use capability::CapabilityIndex;
