//! FlexibleChain work-stealing pool — crossbeam-deque Chase-Lev deques
//! scoped per `CapabilityDecl`.
//!
//! Per COOPERATION.md §20.4: "Any capable agent can pick up the next step."
//! DRG mineral-mining style. The per-capability partitioning means each
//! pool contains only agents that hold the relevant capability (typically
//! 5–50 out of 1000+ formation members), keeping steal-miss iteration
//! bounded.
//!
//! Per Rayon / crossbeam's own scheduler: each worker drains its local
//! deque first, falls back to the per-capability injector, and only then
//! steals from peer workers. This matches the "locality first, balance
//! second" discipline RTS units need at scale.

use std::collections::HashMap;
use std::iter;
use std::sync::Mutex;

use crossbeam_deque::{Injector, Stealer, Worker};

use crate::cadence::AgentId;
use crate::capability::CapabilityDecl;

use super::HandoffPayload;

/// Per-capability work pool. One `Injector` holds unclaimed payloads;
/// each registered agent has its own `Worker` deque plus a `Stealer`
/// cloned into every peer's steal ring for cross-agent balancing.
pub struct FlexibleChainPool {
    inner: Mutex<PoolInner>,
}

struct PoolInner {
    injectors: HashMap<CapabilityDecl, Injector<HandoffPayload>>,
    workers: HashMap<(CapabilityDecl, AgentId), Worker<HandoffPayload>>,
    stealers: HashMap<CapabilityDecl, Vec<Stealer<HandoffPayload>>>,
}

impl FlexibleChainPool {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(PoolInner {
                injectors: HashMap::new(),
                workers: HashMap::new(),
                stealers: HashMap::new(),
            }),
        }
    }

    /// Register an agent as a worker for a capability. Idempotent per
    /// `(capability, agent)` pair — re-registering the same pair is a no-op.
    pub fn register(&self, cap: CapabilityDecl, agent: AgentId) {
        let Ok(mut g) = self.inner.lock() else {
            return;
        };
        g.injectors.entry(cap.clone()).or_insert_with(Injector::new);
        if g.workers.contains_key(&(cap.clone(), agent)) {
            return;
        }
        let worker: Worker<HandoffPayload> = Worker::new_fifo();
        let stealer = worker.stealer();
        g.stealers.entry(cap.clone()).or_default().push(stealer);
        g.workers.insert((cap, agent), worker);
    }

    /// Unregister an agent's worker (on leave / disable).
    pub fn unregister(&self, cap: &CapabilityDecl, agent: AgentId) {
        let Ok(mut g) = self.inner.lock() else {
            return;
        };
        // Workers own a Stealer handle; when the worker is dropped peer
        // stealers start returning Steal::Empty for its remaining work.
        // We can't easily remove one Stealer from the peer Vec without
        // identity — but the extra empty stealer is harmless (a single
        // branch miss per steal attempt). Drop is the real cleanup.
        g.workers.remove(&(cap.clone(), agent));
    }

    /// Post a payload into the capability's global injector queue.
    /// Any registered worker for that capability can claim it.
    pub fn post(&self, cap: &CapabilityDecl, payload: HandoffPayload) {
        let Ok(g) = self.inner.lock() else {
            return;
        };
        if let Some(inj) = g.injectors.get(cap) {
            inj.push(payload);
        } else {
            tracing::debug!(capability = %cap, "no injector registered; dropping payload");
        }
    }

    /// Find work: local worker deque → global injector (bulk steal) →
    /// peer stealers. Returns `None` when all three are empty after one
    /// non-retry pass (preserves crossbeam's `is_retry` semantics so
    /// callers know to back off).
    pub fn find_task(&self, cap: &CapabilityDecl, agent: AgentId) -> Option<HandoffPayload> {
        let Ok(g) = self.inner.lock() else {
            return None;
        };
        let local = g.workers.get(&(cap.clone(), agent))?;
        let injector = g.injectors.get(cap)?;
        let stealers = g.stealers.get(cap)?;

        if let Some(task) = local.pop() {
            return Some(task);
        }
        iter::repeat_with(|| {
            injector
                .steal_batch_and_pop(local)
                .or_else(|| stealers.iter().map(|s| s.steal()).collect())
        })
        .find(|s| !s.is_retry())
        .and_then(|s| s.success())
    }

    /// Number of agents registered under any capability — useful for
    /// observability.
    pub fn registered_count(&self) -> usize {
        self.inner.lock().map(|g| g.workers.len()).unwrap_or(0)
    }
}

impl Default for FlexibleChainPool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::cadence::ActionDescriptor;

    fn payload(tag: &str) -> HandoffPayload {
        HandoffPayload {
            data: serde_json::json!({ "tag": tag }),
            schema: "test".into(),
            produced_by: ActionDescriptor {
                kind: tag.to_owned(),
                target: None,
                payload_hash: 0,
            },
            consumable_by: vec![],
            expires: None,
        }
    }

    #[test]
    fn post_then_find_returns_payload() {
        let pool = FlexibleChainPool::new();
        let cap: CapabilityDecl = "github".into();
        let agent = AgentId::new();
        pool.register(cap.clone(), agent);
        pool.post(&cap, payload("p1"));
        let got = pool.find_task(&cap, agent).expect("should find");
        assert_eq!(got.data["tag"], "p1");
    }

    #[test]
    fn multiple_workers_steal_from_peer() {
        let pool = FlexibleChainPool::new();
        let cap: CapabilityDecl = "slack".into();
        let a = AgentId::new();
        let b = AgentId::new();
        pool.register(cap.clone(), a);
        pool.register(cap.clone(), b);
        // Post two payloads — either worker should pick them up.
        pool.post(&cap, payload("p1"));
        pool.post(&cap, payload("p2"));
        let first = pool.find_task(&cap, a);
        let second = pool.find_task(&cap, b);
        assert!(first.is_some());
        assert!(second.is_some());
        // Third call on empty queue returns None.
        let third = pool.find_task(&cap, a);
        assert!(third.is_none());
    }

    #[test]
    fn unregister_removes_worker() {
        let pool = FlexibleChainPool::new();
        let cap: CapabilityDecl = "discord".into();
        let agent = AgentId::new();
        pool.register(cap.clone(), agent);
        assert_eq!(pool.registered_count(), 1);
        pool.unregister(&cap, agent);
        assert_eq!(pool.registered_count(), 0);
    }

    #[test]
    fn post_without_registration_drops_cleanly() {
        let pool = FlexibleChainPool::new();
        let cap: CapabilityDecl = "unknown".into();
        // No agent registered for this capability — no injector either.
        // post() logs and returns.
        pool.post(&cap, payload("p1"));
        // find_task returns None because worker lookup fails.
        let agent = AgentId::new();
        assert!(pool.find_task(&cap, agent).is_none());
    }
}
