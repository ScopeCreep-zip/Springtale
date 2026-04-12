//! Cadence system — shared tick bus for formation synchronization.
//!
//! Per COOPERATION.pdf §5: "Necrodancer insight: neither player owns
//! the beat. The music IS the clock. All participants synchronize to it."
//!
//! The CadenceBus provides an external clock that all formation members
//! synchronize to. The tick rate can modulate based on pacing phase
//! (§22) — faster during Peak, slower during Recovery.
//!
//! Ryan Clark's discovery: "100% timing leeway felt best." This means
//! agents should have generous windows to commit actions — the hard
//! part is choosing the RIGHT action, not hitting the timing.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::{RwLock, broadcast};

/// Unique identifier for an agent in a formation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgentId(pub uuid::Uuid);

impl Default for AgentId {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }
}

/// What the formation should accomplish (not how).
///
/// Per COOPERATION.pdf §3.2: "Intent describes WHAT, never HOW.
/// 'Attack' tells the formation to engage. It does not tell individual
/// agents which target to pick, what timing to use, or what sequence
/// to follow."
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IntentPattern {
    /// Gather information. Sensor agents activate.
    /// Patapon: PATA PATA PATA PON. Siege: Drone phase.
    Reconnoiter { target: String },

    /// Execute against a known target.
    /// Patapon: PON PON PATA PON. Total War: Charge.
    Execute { plan_id: Option<String> },

    /// Hold current state. Defensive agents activate.
    /// Patapon: CHAKA CHAKA PATA PON. Total War: Guard mode.
    Stabilize { reason: String },

    /// Maximum commitment to singular objective.
    /// Patapon: DON DON CHAKA CHAKA. Army of Two: Overkill.
    Surge { objective: String },

    /// Graceful wind-down.
    Dissolve { reason: String },
}

/// A single tick of the cadence bus.
#[derive(Debug, Clone)]
pub struct Tick {
    /// Monotonically increasing tick number.
    pub sequence: u64,
    /// When this tick was emitted.
    pub timestamp: Instant,
    /// Current intent pattern for the formation.
    pub intent: IntentPattern,
    /// How long agents have to respond (generous, per Necrodancer insight).
    pub window: Duration,
}

/// What an agent did during a tick.
#[derive(Debug, Clone)]
pub struct TickReport {
    /// Which agent produced this report.
    pub agent_id: AgentId,
    /// Which tick this report covers.
    pub tick_sequence: u64,
    /// What action the agent took (None = idle/skipped).
    pub action_taken: Option<String>,
    /// How long the agent took to respond.
    pub latency: Duration,
    /// How well the action aligned with the current intent (0.0-1.0).
    pub intent_alignment: f32,
    /// Agents this action interfered with (Helldivers friendly fire).
    pub interference_with: Vec<AgentId>,
}

/// The shared tick bus that all formation members synchronize to.
///
/// Per Necrodancer: the clock is external and environmental.
/// Per Patapon: the rhythm encodes intent (different drum patterns
/// for march, attack, defend, charge).
#[derive(Clone)]
pub struct CadenceBus {
    /// Time between ticks.
    pub tick_interval: Duration,
    /// Current intent pattern broadcast to all members.
    pub current_intent: Arc<RwLock<IntentPattern>>,
    /// Monotonically increasing tick counter.
    pub tick_counter: Arc<AtomicU64>,
    /// Broadcast channel for ticks.
    pub tx: broadcast::Sender<Tick>,
}

impl CadenceBus {
    /// Create a new cadence bus with the given tick interval.
    pub fn new(tick_interval: Duration, tx: broadcast::Sender<Tick>) -> Self {
        Self {
            tick_interval,
            current_intent: Arc::new(RwLock::new(IntentPattern::Stabilize {
                reason: "formation assembling".to_owned(),
            })),
            tick_counter: Arc::new(AtomicU64::new(0)),
            tx,
        }
    }

    /// Run the cadence bus — emits ticks at the configured interval.
    ///
    /// This runs as a background tokio task. Call from BotBuilder::build().
    pub async fn run(&self) {
        let mut interval = tokio::time::interval(self.tick_interval);

        loop {
            interval.tick().await;

            let sequence = self.tick_counter.fetch_add(1, Ordering::Relaxed);
            let intent = self.current_intent.read().await.clone();

            let tick = Tick {
                sequence,
                timestamp: Instant::now(),
                intent,
                window: self.tick_interval, // generous window per Necrodancer insight
            };

            // Broadcast to all subscribers. If none, that's fine.
            let _ = self.tx.send(tick);
        }
    }

    /// Subscribe to ticks. Each formation gets its own receiver.
    pub fn subscribe(&self) -> broadcast::Receiver<Tick> {
        self.tx.subscribe()
    }

    /// Get the current tick count.
    pub fn tick_count(&self) -> u64 {
        self.tick_counter.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cadence_bus_creation() {
        let (tx, _rx) = broadcast::channel(16);
        let bus = CadenceBus::new(Duration::from_millis(100), tx);
        assert_eq!(bus.tick_count(), 0);
    }

    #[tokio::test]
    async fn test_cadence_bus_subscribe() {
        let (tx, _rx) = broadcast::channel(16);
        let bus = CadenceBus::new(Duration::from_millis(50), tx);
        let mut rx = bus.subscribe();

        // Spawn the bus
        let bus_clone = bus.clone();
        let handle = tokio::spawn(async move {
            bus_clone.run().await;
        });

        // Receive a tick
        let tick = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("timeout waiting for tick")
            .expect("channel closed");

        assert_eq!(tick.sequence, 0);
        handle.abort();
    }

    #[tokio::test]
    async fn test_intent_change() {
        let (tx, _rx) = broadcast::channel(16);
        let bus = CadenceBus::new(Duration::from_millis(100), tx);

        // Change intent
        *bus.current_intent.write().await = IntentPattern::Execute { plan_id: None };

        let intent = bus.current_intent.read().await;
        assert!(matches!(&*intent, IntentPattern::Execute { .. }));
    }
}
