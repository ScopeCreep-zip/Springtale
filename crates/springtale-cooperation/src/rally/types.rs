//! Rally & cascade recovery — formation self-healing before orchestrator escalation.
//!
//! Per COOPERATION.md §15:
//! Game sources: Total War general rally, routing cascade, Monster Hunter carts.
//!
//! §15.1 Cascade Detection: Agent A fails → neighbors see it → their health drops → cascade risk.
//! §15.2 Formation Self-Rally (before escalating to orchestrator):
//!   1. Redistribute attention (§9) away from struggling agent
//!   2. Transform roles (§14) for failed agent
//!   3. Reduce momentum tier to match reduced coherence
//!   4. Consume rally token (limited, like Monster Hunter carts)
//!
//! §15.3 Escalation: Only if self-rally fails (tokens consumed, Cold momentum,
//! multiple agents failing) does the formation escalate to orchestrator::intervention.
//!
//! Per spec §15.3 the rally primitive is `Arc<Semaphore>` for token
//! accounting (member liveness is read from each member's in-flight
//! dispatch by the bot's supervision step, plan 1.8):
//!
//! - `Semaphore::try_acquire_owned()` plus `OwnedSemaphorePermit::forget()`
//!   gives us the Monster Hunter cart semantic ("cart consumed, doesn't
//!   come back"). Plain drop would recycle the permit, which is wrong.
//! - `Semaphore::close()` is the escalation latch: once closed, every
//!   subsequent consume fails with `Closed` and every waiter wakes — a
//!   single primitive replaces an atomic counter + a manual notifier.

use std::sync::Arc;

use thiserror::Error;
use tokio::sync::{Semaphore, TryAcquireError, broadcast};

use crate::cadence::AgentId;

/// Rally attempt result.
#[derive(Debug, Clone)]
pub enum RallyResult {
    /// Formation self-recovered successfully.
    Recovered,
    /// Rally token consumed but formation stabilized.
    StabilizedWithCost { tokens_remaining: u32 },
    /// Self-rally failed — escalate to orchestrator::intervention.
    EscalateToOrchestrator { reason: String },
}

/// Lifecycle events during a rally attempt (spec §15).
///
/// Used by the cascade detector and the event-loop's rally handler to
/// drive logging, state transitions, and escalation decisions.
#[derive(Debug, Clone)]
pub enum RallyEvent {
    /// An agent went down — rally eligible.
    PeerDown { agent: AgentId },
    /// Attention was redistributed away from a failing agent.
    AttentionRedistributed { from: AgentId },
    /// A role transformation was applied as part of self-recovery.
    RoleTransformed { agent: AgentId },
    /// A rally token was consumed.
    TokenConsumed { remaining: u32 },
    /// Self-rally exhausted — escalating to orchestrator intervention.
    Escalated { reason: String },
}

#[derive(Debug, Error)]
pub enum RallyFailure {
    #[error("rally tokens exhausted")]
    NoTokensLeft,
    #[error("rally semaphore closed — formation dissolving")]
    Closed,
}

/// Monster Hunter cart semantic: each `consume()` takes a permit
/// permanently (`forget`), `close()` latches the whole pool as
/// exhausted. The `Arc<Semaphore>` is cloneable so the rally
/// primitive can be shared across the cascade handler and the
/// supervisor loop without a `&mut` bottleneck.
#[derive(Clone)]
pub struct RallyTokens {
    inner: Arc<Semaphore>,
    max: usize,
}

impl RallyTokens {
    pub fn new(budget: usize) -> Self {
        Self {
            inner: Arc::new(Semaphore::new(budget)),
            max: budget,
        }
    }

    /// Consume a token permanently. Returns `Err(NoTokensLeft)` when the
    /// budget is exhausted and `Err(Closed)` after `close()`.
    pub fn consume(&self) -> Result<(), RallyFailure> {
        match self.inner.clone().try_acquire_owned() {
            Ok(permit) => {
                permit.forget();
                Ok(())
            }
            Err(TryAcquireError::NoPermits) => Err(RallyFailure::NoTokensLeft),
            Err(TryAcquireError::Closed) => Err(RallyFailure::Closed),
        }
    }

    /// Current remaining tokens — used by persistence and the UI.
    pub fn remaining(&self) -> usize {
        self.inner.available_permits()
    }

    /// Budget the pool was created with.
    pub fn max(&self) -> usize {
        self.max
    }

    pub fn can_rally(&self) -> bool {
        self.remaining() > 0 && !self.inner.is_closed()
    }

    /// Escalation latch: every future `consume` returns `Err(Closed)`
    /// and any `acquire().await` waiter wakes with a `Closed` error.
    /// Idempotent — safe to call more than once.
    pub fn close(&self) {
        self.inner.close();
    }
}

impl std::fmt::Debug for RallyTokens {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RallyTokens")
            .field("remaining", &self.remaining())
            .field("max", &self.max)
            .field("closed", &self.inner.is_closed())
            .finish()
    }
}

/// Formation rally runtime: the token budget plus the rally event bus.
pub struct FormationRally {
    pub tokens: RallyTokens,
    pub events: broadcast::Sender<RallyEvent>,
    default_events_rx: broadcast::Receiver<RallyEvent>,
}

impl FormationRally {
    pub fn new(token_budget: usize, event_cap: usize) -> Self {
        let (events, default_events_rx) = broadcast::channel(event_cap.max(1));
        Self {
            tokens: RallyTokens::new(token_budget),
            events,
            default_events_rx,
        }
    }

    /// Drain events that landed on the default receiver. Returns the
    /// consumed event list. Callers that want continuous observation
    /// should subscribe with [`subscribe`] and poll their own receiver
    /// — `drain_events` is the synchronous-tick "catch up since last
    /// time" hook.
    pub fn drain_events(&mut self) -> Vec<RallyEvent> {
        let mut out = Vec::new();
        loop {
            match self.default_events_rx.try_recv() {
                Ok(ev) => out.push(ev),
                Err(broadcast::error::TryRecvError::Empty) => break,
                Err(broadcast::error::TryRecvError::Lagged(_)) => {
                    // We are the default receiver — lag means the ring
                    // buffer rolled over before this tick got to drain.
                    // Continue from the new cursor.
                }
                Err(broadcast::error::TryRecvError::Closed) => break,
            }
        }
        out
    }

    /// Spawn a member task under supervision. Stores the abort handle so
    /// planned leaves (`leave(agent)`) can cancel without the supervisor
    /// counting it as a failure.
    pub fn subscribe(&self) -> broadcast::Receiver<RallyEvent> {
        self.events.subscribe()
    }

    /// Restore token state from persistence. Called by
    /// `lifecycle::spawn_formation` when a formation is materialized from
    /// DB with `tokens_remaining < max_tokens`. Consumes `(max - remaining)`
    /// permits up front so the Semaphore reflects disk state.
    pub fn restore_tokens(&self, remaining: usize) {
        let consumed_on_disk = self.tokens.max.saturating_sub(remaining);
        for _ in 0..consumed_on_disk {
            // Ignore result: if budget is 0 and disk says 0, no-op.
            let _ = self.tokens.consume();
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn consume_decrements_remaining() {
        let tokens = RallyTokens::new(3);
        assert_eq!(tokens.remaining(), 3);
        tokens.consume().unwrap();
        assert_eq!(tokens.remaining(), 2);
        tokens.consume().unwrap();
        tokens.consume().unwrap();
        assert!(matches!(tokens.consume(), Err(RallyFailure::NoTokensLeft)));
    }

    #[test]
    fn close_prevents_further_consume() {
        let tokens = RallyTokens::new(3);
        tokens.close();
        assert!(matches!(tokens.consume(), Err(RallyFailure::Closed)));
        assert!(!tokens.can_rally());
    }

    #[test]
    fn forget_makes_consume_permanent() {
        let tokens = RallyTokens::new(2);
        tokens.consume().unwrap();
        // Dropping wouldn't return the permit — `consume` used `forget`.
        assert_eq!(tokens.remaining(), 1);
    }

    #[test]
    fn restore_tokens_reproduces_disk_state() {
        let rally = FormationRally::new(3, 8);
        rally.restore_tokens(1); // disk says 1 of 3 remain
        assert_eq!(rally.tokens.remaining(), 1);
        assert_eq!(rally.tokens.max(), 3);
    }
}
