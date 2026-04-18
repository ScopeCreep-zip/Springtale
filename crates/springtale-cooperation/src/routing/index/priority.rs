use std::collections::BinaryHeap;
use std::sync::Mutex;

use dashmap::DashMap;

use crate::capability::CapabilityDecl;
use crate::routing::types::PriorityTask;

/// Return the highest-priority task visible across the given capability buckets
/// without removing it from the heap. Clones the peek; removal is caller-driven
/// after a successful claim (RimWorld JobGiver_Work semantics — scan then
/// reserve).
pub(super) fn best_across(
    buckets: &DashMap<String, Mutex<BinaryHeap<PriorityTask>>>,
    capabilities: &[CapabilityDecl],
) -> Option<PriorityTask> {
    let mut best: Option<PriorityTask> = None;
    for cap in capabilities {
        if let Some(bucket) = buckets.get(&cap.name)
            && let Ok(heap) = bucket.lock()
            && let Some(top) = heap.peek()
            && best.as_ref().is_none_or(|b| top > b)
        {
            best = Some(top.clone());
        }
    }
    best
}
