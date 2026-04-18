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

impl std::fmt::Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
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

/// Describes what action an agent took during a tick.
///
/// Per COOPERATION_IMPLEMENTATION_PLAN.md §5.4: typed struct with kind
/// and target for meaningful interference detection. Two agents both
/// doing "write" to different targets isn't a conflict; two agents
/// writing the SAME target is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionDescriptor {
    /// Action kind (e.g., "send_message", "write_file", "read_issues").
    pub kind: String,
    /// Action target (e.g., key, channel, path). Interference is detected
    /// when two agents act on the same target.
    pub target: Option<String>,
    /// Hash of the action payload for redundancy detection.
    pub payload_hash: u64,
}

/// What an agent did during a tick.
#[derive(Debug, Clone)]
pub struct TickReport {
    /// Which agent produced this report.
    pub agent_id: AgentId,
    /// Which tick this report covers.
    pub tick_sequence: u64,
    /// What action the agent took (None = idle/skipped).
    pub action_taken: Option<ActionDescriptor>,
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
/// The shared tick bus. One per formation. Wrap in `Arc` for cloning
/// across tasks (spec §5.4 test shows `Arc::new(bus)` pattern).
pub struct CadenceBus {
    pub tick_interval: Duration,
    pub current_intent: Arc<RwLock<IntentPattern>>,
    tick_counter: AtomicU64,
    tx: broadcast::Sender<Tick>,
    reports_tx: tokio::sync::mpsc::Sender<TickReport>,
}

impl CadenceBus {
    /// Create a new bus with the given tick interval and channel capacity.
    /// Per spec §5.4: creates broadcast + reports channels internally.
    pub fn new(
        tick_interval: Duration,
        capacity: usize,
    ) -> (Self, tokio::sync::mpsc::Receiver<TickReport>) {
        let (tx, _seed) = broadcast::channel(capacity);
        let (reports_tx, reports_rx) = tokio::sync::mpsc::channel(capacity * 4);
        let bus = Self {
            tick_interval,
            current_intent: Arc::new(RwLock::new(IntentPattern::Stabilize {
                reason: "formation assembling".to_owned(),
            })),
            tick_counter: AtomicU64::new(0),
            tx,
            reports_tx,
        };
        (bus, reports_rx)
    }

    /// Sensible default: 30 Hz, 256 tick backlog.
    pub fn default_30hz() -> (Self, tokio::sync::mpsc::Receiver<TickReport>) {
        Self::new(Duration::from_millis(33), 256)
    }

    /// Subscribe a new agent to the tick stream.
    pub fn subscribe(&self) -> broadcast::Receiver<Tick> {
        self.tx.subscribe()
    }

    /// Reports channel sender — clone and pass to agents so they can
    /// report back after each tick.
    pub fn reports_sender(&self) -> tokio::sync::mpsc::Sender<TickReport> {
        self.reports_tx.clone()
    }

    /// Change the current intent broadcast on the next tick.
    pub async fn set_intent(&self, intent: IntentPattern) {
        let mut guard = self.current_intent.write().await;
        *guard = intent;
    }

    /// Get the current tick count.
    pub fn tick_count(&self) -> u64 {
        self.tick_counter.load(Ordering::Relaxed)
    }

    /// Main loop — call from a `tokio::spawn` that owns the bus via Arc.
    /// Per spec §5.4: runs until all subscribers drop.
    pub async fn run(&self) {
        let mut interval = tokio::time::interval(self.tick_interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            interval.tick().await;

            let intent = self.current_intent.read().await.clone();
            let seq = self.tick_counter.fetch_add(1, Ordering::Relaxed);
            let tick = Tick {
                sequence: seq,
                timestamp: Instant::now(),
                intent,
                window: self.tick_interval.saturating_mul(4),
            };

            if self.tx.send(tick).is_err() {
                tracing::debug!("cadence bus: all subscribers dropped, stopping");
                return;
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cadence_bus_subscribe_and_receive_tick() {
        let (bus, mut reports_rx) = CadenceBus::new(Duration::from_millis(50), 16);
        let mut tick_rx = bus.subscribe();
        let sender = bus.reports_sender();
        let bus = Arc::new(bus);

        let bus_run = bus.clone();
        let handle = tokio::spawn(async move {
            bus_run.run().await;
        });

        // Receive a tick from the bus
        let tick = tokio::time::timeout(Duration::from_secs(1), tick_rx.recv())
            .await
            .expect("timeout waiting for tick")
            .expect("channel closed");
        assert_eq!(tick.sequence, 0);

        // Send a report back through the reports channel
        sender
            .send(TickReport {
                agent_id: AgentId::new(),
                tick_sequence: tick.sequence,
                action_taken: Some(ActionDescriptor {
                    kind: "send_message".to_owned(),
                    target: Some("slack".to_owned()),
                    payload_hash: 42,
                }),
                latency: Duration::from_millis(5),
                intent_alignment: 0.95,
                interference_with: vec![],
            })
            .await
            .expect("send report");

        // Verify the report arrived on the receiver end
        let report = reports_rx.recv().await.expect("recv report");
        assert_eq!(report.tick_sequence, tick.sequence);
        assert_eq!(report.action_taken.as_ref().unwrap().kind, "send_message");

        handle.abort();
    }

    #[tokio::test]
    async fn test_set_intent() {
        let (bus, mut reports_rx) = CadenceBus::new(Duration::from_millis(100), 16);

        bus.set_intent(IntentPattern::Execute { plan_id: None }).await;

        let intent = bus.current_intent.read().await;
        assert!(matches!(&*intent, IntentPattern::Execute { .. }));

        // Verify the reports channel works in this bus too
        let sender = bus.reports_sender();
        sender
            .send(TickReport {
                agent_id: AgentId::new(),
                tick_sequence: 0,
                action_taken: None,
                latency: Duration::from_millis(0),
                intent_alignment: 1.0,
                interference_with: vec![],
            })
            .await
            .expect("send");
        let report = reports_rx.recv().await.expect("recv");
        assert!((report.intent_alignment - 1.0).abs() < f32::EPSILON);
    }

    #[tokio::test]
    async fn test_default_30hz() {
        let (bus, mut reports_rx) = CadenceBus::default_30hz();
        assert_eq!(bus.tick_interval, Duration::from_millis(33));

        // Verify both channels are functional
        let sender = bus.reports_sender();
        sender
            .send(TickReport {
                agent_id: AgentId::new(),
                tick_sequence: 0,
                action_taken: None,
                latency: Duration::from_millis(0),
                intent_alignment: 0.5,
                interference_with: vec![],
            })
            .await
            .expect("send");
        let report = reports_rx.recv().await.expect("recv");
        assert!((report.intent_alignment - 0.5).abs() < f32::EPSILON);
    }
}
