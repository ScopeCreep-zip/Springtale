//! FormationBus — real tokio channel matrix for inter-agent communication.
//!
//! Per COOPERATION.md §19 channel matrix:
//!
//! | Channel             | Primitive         | Use Case                          |
//! |---------------------|-------------------|-----------------------------------|
//! | StateBroadcast      | broadcast::channel| L4D callouts (lag-drop OK)        |
//! | ProtocolMessage     | mpsc::channel     | Typed cross-context (no loss)     |
//! | DirectionalSignal   | watch::channel    | DRG laser, Siege ping (latest)    |
//! | CohesionSignal      | broadcast::channel| Rock and Stone (morale event)     |
//! | IntentAcknowledgment| mpsc::channel     | Patapon sing-back (reliable)      |
//! | ImplicitSignal      | watch<HashMap>    | Overcooked (state observation)    |
//!
//! Protocol messages and intent acknowledgments must be lossless — per
//! spec §19.1 these have "no loss" semantics. The implementation uses a
//! fan-in + fan-out router: every member holds a clone of one MPSC
//! sender; a dispatcher task owns the receiver and forwards each message
//! to per-member mpsc inboxes keyed in a DashMap.
//!
//! - O(N) channels (one per member), not O(N²)
//! - Subscribe/unsubscribe are sync (DashMap is lock-free for reads)
//! - Per-inbox fullness causes per-inbox drop (an unresponsive member
//!   doesn't block traffic to healthy peers)

use std::collections::HashMap;
use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::{broadcast, mpsc, watch};

use crate::cadence::{ActionDescriptor, AgentId, IntentPattern};

use super::{BroadcastTrigger, CommChannel, MessageTarget, ProtocolPayload, StateMessage};

pub const STATE_BROADCAST_CAP: usize = 64;
pub const COHESION_CAP: usize = 32;
pub const PROTOCOL_ROUTER_CAP: usize = 1024; // fan-in buffer
pub const PROTOCOL_INBOX_CAP: usize = 128; // per-member fan-out
pub const INTENT_ACK_CAP: usize = 256; // fan-in to cadence

/// State broadcast message — L4D automatic callouts.
#[derive(Clone, Debug)]
pub struct StateBroadcastMsg {
    pub source: AgentId,
    pub trigger: BroadcastTrigger,
    pub message: StateMessage,
}

/// Protocol message — typed cross-context communication.
/// Sent via fan-in mpsc; dispatcher routes to per-member inbox by target.
#[derive(Clone, Debug)]
pub struct ProtocolMsg {
    pub source: AgentId,
    pub target: MessageTarget,
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

/// Intent acknowledgment — Patapon sing-back. Fan-in mpsc to the formation's
/// cadence evaluator (single consumer).
#[derive(Clone, Debug)]
pub struct IntentAckMsg {
    pub source: AgentId,
    pub intent_confirmed: IntentPattern,
    pub interpretation: String,
}

/// Shared map of per-member protocol inboxes. `DashMap` lets the dispatcher
/// read and subscribe/unsubscribe all happen without async locks.
pub type ProtocolInboxes = Arc<DashMap<AgentId, mpsc::Sender<ProtocolMsg>>>;

/// The multi-layer communication bus for a formation.
pub struct FormationBus {
    /// L4D callouts — automatic state broadcasts. Lag-drop OK.
    pub state_tx: broadcast::Sender<StateBroadcastMsg>,
    /// Rock and Stone — morale/cohesion events. Lag-drop OK.
    pub cohesion_tx: broadcast::Sender<CohesionSignalMsg>,
    /// DRG laser pointer — latest directional signal only.
    pub directional_tx: watch::Sender<Option<DirectionalSignalMsg>>,
    /// Fan-in end for typed protocol messages. Dispatcher owns the receiver.
    pub protocol_tx: mpsc::Sender<ProtocolMsg>,
    /// Per-member inboxes populated by `subscribe(agent_id)`.
    pub protocol_inboxes: ProtocolInboxes,
    /// Fan-in acks to the cadence evaluator (single consumer owns the rx).
    pub intent_ack_tx: mpsc::Sender<IntentAckMsg>,
    /// Overcooked implicit signals — observable agent state.
    pub implicit_tx: watch::Sender<HashMap<AgentId, ActionDescriptor>>,
}

/// Receivers given to a single agent for its per-message consumption.
pub struct FormationBusSubscription {
    pub agent_id: AgentId,
    pub state_rx: broadcast::Receiver<StateBroadcastMsg>,
    pub cohesion_rx: broadcast::Receiver<CohesionSignalMsg>,
    pub directional_rx: watch::Receiver<Option<DirectionalSignalMsg>>,
    /// Per-member inbox: the dispatcher places targeted ProtocolMsgs here.
    pub protocol_rx: mpsc::Receiver<ProtocolMsg>,
    pub implicit_rx: watch::Receiver<HashMap<AgentId, ActionDescriptor>>,
}

/// Protocol dispatcher end — owned by the fan-out task.
pub struct ProtocolDispatch {
    pub rx: mpsc::Receiver<ProtocolMsg>,
    pub inboxes: ProtocolInboxes,
}

/// Ack consumer end — owned by the cadence evaluator task.
pub struct AckDispatch {
    pub rx: mpsc::Receiver<IntentAckMsg>,
}

impl FormationBus {
    /// Create a new formation bus. Returns the bus (senders + inbox map)
    /// plus the two dispatcher ends. Each dispatch end is consumed by a
    /// `tokio::spawn` loop (see `dispatcher::protocol::run` and
    /// `dispatcher::ack::run`).
    pub fn new() -> (Self, ProtocolDispatch, AckDispatch) {
        let (state_tx, _) = broadcast::channel(STATE_BROADCAST_CAP);
        let (cohesion_tx, _) = broadcast::channel(COHESION_CAP);
        let (directional_tx, _) = watch::channel(None);
        let (protocol_tx, protocol_rx) = mpsc::channel(PROTOCOL_ROUTER_CAP);
        let (intent_ack_tx, intent_ack_rx) = mpsc::channel(INTENT_ACK_CAP);
        let (implicit_tx, _) = watch::channel(HashMap::new());
        let protocol_inboxes: ProtocolInboxes = Arc::new(DashMap::new());

        let bus = Self {
            state_tx,
            cohesion_tx,
            directional_tx,
            protocol_tx,
            protocol_inboxes: Arc::clone(&protocol_inboxes),
            intent_ack_tx,
            implicit_tx,
        };
        let proto = ProtocolDispatch {
            rx: protocol_rx,
            inboxes: protocol_inboxes,
        };
        let ack = AckDispatch { rx: intent_ack_rx };
        (bus, proto, ack)
    }

    /// Subscribe a member. Allocates a fresh per-agent protocol inbox and
    /// registers its sender end with the dispatcher. Sync — no async lock.
    pub fn subscribe(&self, agent_id: AgentId) -> FormationBusSubscription {
        let (proto_in_tx, proto_in_rx) = mpsc::channel(PROTOCOL_INBOX_CAP);
        self.protocol_inboxes.insert(agent_id, proto_in_tx);
        FormationBusSubscription {
            agent_id,
            state_rx: self.state_tx.subscribe(),
            cohesion_rx: self.cohesion_tx.subscribe(),
            directional_rx: self.directional_tx.subscribe(),
            protocol_rx: proto_in_rx,
            implicit_rx: self.implicit_tx.subscribe(),
        }
    }

    /// Remove a member's inbox (used on detach / down).
    pub fn unsubscribe(&self, agent_id: AgentId) {
        self.protocol_inboxes.remove(&agent_id);
    }

    /// Current number of subscribed inboxes — useful for LiveFormationReader
    /// health indicators.
    pub fn inbox_count(&self) -> usize {
        self.protocol_inboxes.len()
    }

    pub fn broadcast_state(&self, msg: StateBroadcastMsg) {
        let _ = self.state_tx.send(msg);
    }

    pub fn signal_cohesion(&self, msg: CohesionSignalMsg) {
        let _ = self.cohesion_tx.send(msg);
    }

    pub fn point_at(&self, msg: DirectionalSignalMsg) {
        let _ = self.directional_tx.send(Some(msg));
    }

    pub fn clear_direction(&self) {
        let _ = self.directional_tx.send(None);
    }

    pub fn update_implicit(&self, agent: AgentId, action: ActionDescriptor) {
        self.implicit_tx.send_modify(|map| {
            map.insert(agent, action);
        });
    }

    /// Taxonomy-aware dispatch entry point — matches spec §19.1 channel
    /// matrix exhaustively, so every `CommChannel` variant routes to the
    /// right underlying primitive. Preferred over touching the typed
    /// senders directly because this enforces the matrix at compile time.
    pub async fn send(&self, ch: CommChannel) -> Result<(), ChannelSendError> {
        match ch {
            CommChannel::StateBroadcast {
                source,
                condition,
                message,
            } => {
                let _ = self.state_tx.send(StateBroadcastMsg {
                    source,
                    trigger: condition,
                    message,
                });
                Ok(())
            }
            CommChannel::ProtocolMessage {
                source,
                target,
                message,
            } => self
                .protocol_tx
                .send(ProtocolMsg {
                    source,
                    target,
                    payload: message,
                })
                .await
                .map_err(|_| ChannelSendError::ProtocolRouterClosed),
            CommChannel::DirectionalSignal {
                source,
                target_object,
                urgency,
            } => {
                let _ = self.directional_tx.send(Some(DirectionalSignalMsg {
                    source,
                    target_object,
                    urgency,
                }));
                Ok(())
            }
            CommChannel::CohesionSignal { source } => {
                let _ = self.cohesion_tx.send(CohesionSignalMsg { source });
                Ok(())
            }
            CommChannel::IntentAcknowledgment {
                source,
                intent_confirmed,
                interpretation,
            } => self
                .intent_ack_tx
                .send(IntentAckMsg {
                    source,
                    intent_confirmed,
                    interpretation,
                })
                .await
                .map_err(|_| ChannelSendError::AckRouterClosed),
            CommChannel::ImplicitSignal {
                source,
                observed_action,
                inferred_meaning: _,
            } => {
                self.update_implicit(source, observed_action);
                Ok(())
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ChannelSendError {
    #[error("protocol router closed (dispatcher dropped)")]
    ProtocolRouterClosed,
    #[error("ack router closed (cadence evaluator dropped)")]
    AckRouterClosed,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn state_broadcast_fans_out_to_subscribers() {
        let (bus, _proto, _ack) = FormationBus::new();
        let agent = AgentId::new();
        let mut sub = bus.subscribe(agent);

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
    async fn cohesion_signal_reaches_subscribers() {
        let (bus, _p, _a) = FormationBus::new();
        let agent = AgentId::new();
        let mut sub = bus.subscribe(agent);

        bus.signal_cohesion(CohesionSignalMsg { source: agent });

        let msg = sub.cohesion_rx.recv().await.unwrap();
        assert_eq!(msg.source, agent);
    }

    #[tokio::test]
    async fn directional_signal_latest_only() {
        let (bus, _p, _a) = FormationBus::new();
        let agent = AgentId::new();
        let mut sub = bus.subscribe(agent);

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
    async fn implicit_signal_observable() {
        let (bus, _p, _a) = FormationBus::new();
        let agent = AgentId::new();
        let mut sub = bus.subscribe(agent);

        bus.update_implicit(
            agent,
            ActionDescriptor {
                kind: "processing".to_owned(),
                target: Some("queue".to_owned()),
                payload_hash: 42,
            },
        );

        sub.implicit_rx.changed().await.unwrap();
        let state = sub.implicit_rx.borrow().clone();
        assert!(state.contains_key(&agent));
        assert_eq!(state[&agent].kind, "processing");
    }

    #[tokio::test]
    async fn protocol_message_fan_in_fan_out_via_dispatcher() {
        use super::super::dispatcher::protocol as dp;

        let (bus, proto_dispatch, _ack) = FormationBus::new();
        let a = AgentId::new();
        let b = AgentId::new();
        let _sub_a = bus.subscribe(a);
        let mut sub_b = bus.subscribe(b);

        // Spawn the fan-out dispatcher.
        let dispatcher_handle = tokio::spawn(dp::run(proto_dispatch, |_cap| None));

        // a sends a targeted message to b.
        bus.protocol_tx
            .send(ProtocolMsg {
                source: a,
                target: MessageTarget::Specific(b),
                payload: ProtocolPayload {
                    message_type: "status_update".to_owned(),
                    data: serde_json::json!({"status": "ready"}),
                },
            })
            .await
            .unwrap();

        // b receives it via its per-member inbox.
        let msg = sub_b.protocol_rx.recv().await.unwrap();
        assert_eq!(msg.source, a);
        assert_eq!(msg.payload.message_type, "status_update");

        dispatcher_handle.abort();
    }

    #[tokio::test]
    async fn ack_fan_in_reaches_single_consumer() {
        let (bus, _proto, mut ack_dispatch) = FormationBus::new();
        let agent = AgentId::new();

        bus.intent_ack_tx
            .send(IntentAckMsg {
                source: agent,
                intent_confirmed: IntentPattern::Execute { plan_id: None },
                interpretation: "running".to_owned(),
            })
            .await
            .unwrap();

        let ack = ack_dispatch.rx.recv().await.unwrap();
        assert_eq!(ack.source, agent);
    }

    #[tokio::test]
    async fn unsubscribe_removes_inbox() {
        let (bus, _p, _a) = FormationBus::new();
        let agent = AgentId::new();
        let _sub = bus.subscribe(agent);
        assert_eq!(bus.inbox_count(), 1);
        bus.unsubscribe(agent);
        assert_eq!(bus.inbox_count(), 0);
    }

    #[tokio::test]
    async fn send_commchannel_dispatches_cohesion() {
        let (bus, _p, _a) = FormationBus::new();
        let agent = AgentId::new();
        let mut sub = bus.subscribe(agent);

        bus.send(CommChannel::CohesionSignal { source: agent })
            .await
            .unwrap();

        let msg = sub.cohesion_rx.recv().await.unwrap();
        assert_eq!(msg.source, agent);
    }
}
