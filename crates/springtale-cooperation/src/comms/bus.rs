//! FormationBus — real tokio channel matrix for inter-agent communication.
//!
//! Per COOPERATION_IMPLEMENTATION_PLAN.md §19 channel matrix:
//!
//! | Channel             | Primitive         | Use Case                          |
//! |---------------------|-------------------|-----------------------------------|
//! | StateBroadcast      | broadcast::channel| L4D callouts (lag-drop OK)        |
//! | ProtocolMessage     | mpsc::channel     | Typed cross-context (no loss)     |
//! | DirectionalSignal   | watch::channel    | DRG laser, Siege ping (latest)    |
//! | CohesionSignal      | broadcast::channel| Rock and Stone (morale event)     |
//! | IntentAcknowledgment| mpsc::channel     | Patapon sing-back (reliable)      |
//! | ImplicitSignal      | watch<HashMap>    | Overcooked (state observation)    |

use std::collections::HashMap;

use tokio::sync::{broadcast, watch};

use crate::cadence::{ActionDescriptor, AgentId, IntentPattern};

use super::{BroadcastTrigger, ProtocolPayload, StateMessage};

/// State broadcast message — L4D automatic callouts.
/// Sent on condition triggers (health low, threat detected), not by agent decision.
#[derive(Clone, Debug)]
pub struct StateBroadcastMsg {
    pub source: AgentId,
    pub trigger: BroadcastTrigger,
    pub message: StateMessage,
}

/// Protocol message — typed cross-context communication.
/// Broadcast to all members; agents filter by target.
#[derive(Clone, Debug)]
pub struct ProtocolMsg {
    pub source: AgentId,
    pub target: super::MessageTarget,
    pub payload: ProtocolPayload,
}

/// Directional signal — attention-directing. Latest-only (watch).
#[derive(Clone, Debug)]
pub struct DirectionalSignalMsg {
    pub source: AgentId,
    pub target_object: String,
    pub urgency: f32,
}

/// Cohesion signal — morale event. No information content.
#[derive(Clone, Debug)]
pub struct CohesionSignalMsg {
    pub source: AgentId,
}

/// Intent acknowledgment — Patapon sing-back. Broadcast to all members.
#[derive(Clone, Debug)]
pub struct IntentAckMsg {
    pub source: AgentId,
    pub intent_confirmed: IntentPattern,
    pub interpretation: String,
}

/// The multi-layer communication bus for a formation.
///
/// Per spec §19: "Every cooperative game solves inter-agent communication
/// differently. The variation reveals that agents need multiple simultaneous
/// communication layers, not a single channel."
///
/// Each channel type uses the tokio primitive best suited to its semantics:
/// - broadcast for fire-and-forget (OK to lag-drop)
/// - mpsc for reliable delivery (protocol messages, intent acks)
/// - watch for latest-value (directional signals, implicit state)
pub struct FormationBus {
    /// L4D callouts — automatic state broadcasts. Lag-drop OK.
    pub state_tx: broadcast::Sender<StateBroadcastMsg>,
    /// Rock and Stone — morale/cohesion events. Lag-drop OK.
    pub cohesion_tx: broadcast::Sender<CohesionSignalMsg>,
    /// DRG laser pointer — latest directional signal only.
    pub directional_tx: watch::Sender<Option<DirectionalSignalMsg>>,
    /// Typed protocol messages — broadcast to all, agents filter by target.
    pub protocol_tx: broadcast::Sender<ProtocolMsg>,
    /// Patapon sing-back — intent acknowledgments. Broadcast to all.
    pub intent_ack_tx: broadcast::Sender<IntentAckMsg>,
    /// Overcooked implicit signals — observable agent state.
    pub implicit_tx: watch::Sender<HashMap<AgentId, ActionDescriptor>>,
}

/// Receivers for the formation bus — one set per subscribing agent.
pub struct FormationBusSubscription {
    pub state_rx: broadcast::Receiver<StateBroadcastMsg>,
    pub cohesion_rx: broadcast::Receiver<CohesionSignalMsg>,
    pub directional_rx: watch::Receiver<Option<DirectionalSignalMsg>>,
    pub protocol_rx: broadcast::Receiver<ProtocolMsg>,
    pub intent_ack_rx: broadcast::Receiver<IntentAckMsg>,
    pub implicit_rx: watch::Receiver<HashMap<AgentId, ActionDescriptor>>,
}

impl FormationBus {
    /// Create a new formation bus with default channel capacities.
    ///
    /// Returns the bus (senders) and initial subscription (receivers).
    /// Additional subscribers call `subscribe()` for broadcast/watch channels.
    /// Protocol and intent_ack channels are point-to-point (one receiver).
    pub fn new() -> (Self, FormationBusSubscription) {
        let (state_tx, state_rx) = broadcast::channel(64);
        let (cohesion_tx, cohesion_rx) = broadcast::channel(32);
        let (directional_tx, directional_rx) = watch::channel(None);
        let (protocol_tx, protocol_rx) = broadcast::channel(128);
        let (intent_ack_tx, intent_ack_rx) = broadcast::channel(64);
        let (implicit_tx, implicit_rx) = watch::channel(HashMap::new());

        let bus = Self {
            state_tx,
            cohesion_tx,
            directional_tx,
            protocol_tx,
            intent_ack_tx,
            implicit_tx,
        };

        let sub = FormationBusSubscription {
            state_rx,
            cohesion_rx,
            directional_rx,
            protocol_rx,
            intent_ack_rx,
            implicit_rx,
        };

        (bus, sub)
    }

    /// Subscribe to all broadcast/watch channels.
    /// Each member gets their own receivers for all 6 channel types.
    pub fn subscribe(&self) -> FormationBusSubscription {
        FormationBusSubscription {
            state_rx: self.state_tx.subscribe(),
            cohesion_rx: self.cohesion_tx.subscribe(),
            directional_rx: self.directional_tx.subscribe(),
            protocol_rx: self.protocol_tx.subscribe(),
            intent_ack_rx: self.intent_ack_tx.subscribe(),
            implicit_rx: self.implicit_tx.subscribe(),
        }
    }

    /// Send a state broadcast (L4D callout).
    pub fn broadcast_state(&self, msg: StateBroadcastMsg) {
        let _ = self.state_tx.send(msg);
    }

    /// Send a cohesion signal (Rock and Stone).
    pub fn signal_cohesion(&self, msg: CohesionSignalMsg) {
        let _ = self.cohesion_tx.send(msg);
    }

    /// Update the directional signal (DRG laser pointer).
    pub fn point_at(&self, msg: DirectionalSignalMsg) {
        let _ = self.directional_tx.send(Some(msg));
    }

    /// Clear the directional signal.
    pub fn clear_direction(&self) {
        let _ = self.directional_tx.send(None);
    }

    /// Update an agent's implicit signal (observable action state).
    pub fn update_implicit(&self, agent: AgentId, action: ActionDescriptor) {
        self.implicit_tx.send_modify(|map| {
            map.insert(agent, action);
        });
    }
}


#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_state_broadcast() {
        let (bus, mut sub) = FormationBus::new();
        let agent = AgentId::new();

        bus.broadcast_state(StateBroadcastMsg {
            source: agent,
            trigger: BroadcastTrigger::HealthBelowThreshold(0.2),
            message: StateMessage {
                content: "low fuel".to_owned(),
                severity: 0.8,
            },
        });

        let msg = sub.state_rx.recv().await.unwrap();
        assert_eq!(msg.source, agent);
    }

    #[tokio::test]
    async fn test_cohesion_signal() {
        let (bus, mut sub) = FormationBus::new();
        let agent = AgentId::new();

        bus.signal_cohesion(CohesionSignalMsg { source: agent });

        let msg = sub.cohesion_rx.recv().await.unwrap();
        assert_eq!(msg.source, agent);
    }

    #[tokio::test]
    async fn test_directional_signal() {
        let (bus, mut sub) = FormationBus::new();
        let agent = AgentId::new();

        bus.point_at(DirectionalSignalMsg {
            source: agent,
            target_object: "issue-42".to_owned(),
            urgency: 0.9,
        });

        sub.directional_rx.changed().await.unwrap();
        let signal = sub.directional_rx.borrow().clone();
        assert!(signal.is_some());
        assert_eq!(signal.as_ref().unwrap().target_object, "issue-42");
    }

    #[tokio::test]
    async fn test_implicit_signal() {
        let (bus, mut sub) = FormationBus::new();
        let agent = AgentId::new();

        bus.update_implicit(agent, ActionDescriptor {
            kind: "processing".to_owned(),
            target: Some("queue".to_owned()),
            payload_hash: 42,
        });

        sub.implicit_rx.changed().await.unwrap();
        let state = sub.implicit_rx.borrow().clone();
        assert!(state.contains_key(&agent));
        assert_eq!(state[&agent].kind, "processing");
    }

    #[tokio::test]
    async fn test_protocol_message() {
        let (bus, mut sub) = FormationBus::new();
        let agent = AgentId::new();

        bus.protocol_tx
            .send(ProtocolMsg {
                source: agent,
                target: super::super::MessageTarget::Formation,
                payload: ProtocolPayload {
                    message_type: "status_update".to_owned(),
                    data: serde_json::json!({"status": "ready"}),
                },
            })
            .unwrap();

        let msg = sub.protocol_rx.recv().await.unwrap();
        assert_eq!(msg.source, agent);
        assert_eq!(msg.payload.message_type, "status_update");
    }

    #[tokio::test]
    async fn test_multiple_subscribers() {
        let (bus, mut sub1) = FormationBus::new();
        let mut sub2 = bus.subscribe();
        let agent = AgentId::new();

        bus.signal_cohesion(CohesionSignalMsg { source: agent });

        let msg1 = sub1.cohesion_rx.recv().await.unwrap();
        let msg2 = sub2.cohesion_rx.recv().await.unwrap();
        assert_eq!(msg1.source, msg2.source);
    }
}
