//! `StateBusSubscriber` — adapter wrapping a `tokio::sync::broadcast::Receiver`
//! of `comms::StateBroadcastMsg` so it implements `dissemination::StateSubscriber`
//! over `dissemination::StateMessage`.
//!
//! Exists because the formation bus and the dissemination StateSubscriber
//! trait carry slightly different message types: the bus broadcasts
//! `StateBroadcastMsg { source, trigger, message }` (per `comms/types.rs`)
//! while the trait emits `StateMessage` variants the agent loop cares about
//! (per `dissemination/state_msg.rs`). This adapter does the translation
//! per message so any caller (member runner, build_reports, tests) can
//! turn a per-member bus subscription into a `StateSubscriber`.

use tokio::sync::broadcast;

use crate::comms::{BroadcastTrigger, StateBroadcastMsg};
use crate::dissemination::trait_::StateSubscriber;
use crate::dissemination::StateMessage;
use crate::types::AgentHealth;

pub struct StateBusSubscriber {
    pub rx: broadcast::Receiver<StateBroadcastMsg>,
}

impl StateBusSubscriber {
    pub fn new(rx: broadcast::Receiver<StateBroadcastMsg>) -> Self {
        Self { rx }
    }
}

impl StateSubscriber for StateBusSubscriber {
    fn try_recv(&mut self) -> Option<StateMessage> {
        // Drain a single bus message and translate. Lagged subscribers
        // skip missed messages (broadcast::error::TryRecvError::Lagged) —
        // matches the per-spec lossy semantics for state broadcasts.
        loop {
            match self.rx.try_recv() {
                Ok(msg) => {
                    if let Some(translated) = translate(msg) {
                        return Some(translated);
                    }
                    // Untranslatable variants are dropped silently;
                    // continue draining so we don't return None for a
                    // queued message that just doesn't map.
                    continue;
                }
                Err(broadcast::error::TryRecvError::Lagged(_)) => continue,
                Err(_) => return None,
            }
        }
    }
}

fn translate(msg: StateBroadcastMsg) -> Option<StateMessage> {
    match msg.trigger {
        BroadcastTrigger::AgentDown(agent) => Some(StateMessage::AgentLeft { agent }),
        BroadcastTrigger::HealthBelowThreshold(_) => Some(StateMessage::AgentHealthChanged {
            agent: msg.source,
            health: AgentHealth::Degraded { recovery_count: 1 },
        }),
        // Other triggers (resource shifts, custom signals) currently
        // have no per-agent awareness mapping; drop them.
        _ => None,
    }
}

/// In-memory buffered variant — wraps a `Vec<StateMessage>` so callers
/// that pre-drain a bus channel into a Vec can apply messages later via
/// the same `StateSubscriber` trait. Used by the in-tick pipeline:
/// `build_reports::run` drains every member's `state_rx` into a HashMap
/// before the per-member loop, then constructs one `BufferedStateSubscriber`
/// per member to drive `react::run`.
pub struct BufferedStateSubscriber {
    pub msgs: std::collections::VecDeque<StateMessage>,
}

impl BufferedStateSubscriber {
    pub fn new(msgs: Vec<StateMessage>) -> Self {
        Self {
            msgs: msgs.into(),
        }
    }
}

impl StateSubscriber for BufferedStateSubscriber {
    fn try_recv(&mut self) -> Option<StateMessage> {
        self.msgs.pop_front()
    }
}

/// Borrowed variant — wraps `&mut broadcast::Receiver<StateBroadcastMsg>`
/// so the in-tick `react` step can drain a member's existing bus
/// subscription without taking ownership. The owned `StateBusSubscriber`
/// is for runner tasks that hold their subscription for their lifetime;
/// the borrowed variant is for the per-tick pipeline that locks
/// `formation.member_subs` for one tick.
pub struct BorrowedStateBusSubscriber<'a> {
    pub rx: &'a mut broadcast::Receiver<StateBroadcastMsg>,
}

impl<'a> BorrowedStateBusSubscriber<'a> {
    pub fn new(rx: &'a mut broadcast::Receiver<StateBroadcastMsg>) -> Self {
        Self { rx }
    }
}

impl StateSubscriber for BorrowedStateBusSubscriber<'_> {
    fn try_recv(&mut self) -> Option<StateMessage> {
        loop {
            match self.rx.try_recv() {
                Ok(msg) => {
                    if let Some(translated) = translate(msg) {
                        return Some(translated);
                    }
                    continue;
                }
                Err(broadcast::error::TryRecvError::Lagged(_)) => continue,
                Err(_) => return None,
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::cadence::AgentId;
    use crate::comms::StateMessage as BusStateMessage;

    fn fab(rx: broadcast::Receiver<StateBroadcastMsg>) -> StateBusSubscriber {
        StateBusSubscriber::new(rx)
    }

    #[test]
    fn agent_down_translates_to_left() {
        let (tx, rx) = broadcast::channel::<StateBroadcastMsg>(8);
        let agent = AgentId::new();
        let _ = tx.send(StateBroadcastMsg {
            source: agent,
            trigger: BroadcastTrigger::AgentDown(agent),
            message: BusStateMessage {
                content: "down".into(),
                severity: 1.0,
            },
        });
        let mut s = fab(rx);
        match s.try_recv() {
            Some(StateMessage::AgentLeft { agent: got }) => assert_eq!(got, agent),
            other => panic!("expected AgentLeft, got {other:?}"),
        }
    }

    #[test]
    fn empty_returns_none() {
        let (_tx, rx) = broadcast::channel::<StateBroadcastMsg>(8);
        let mut s = fab(rx);
        assert!(s.try_recv().is_none());
    }

    #[test]
    fn borrowed_variant_drains_messages() {
        let (tx, mut rx) = broadcast::channel::<StateBroadcastMsg>(8);
        let agent = AgentId::new();
        let _ = tx.send(StateBroadcastMsg {
            source: agent,
            trigger: BroadcastTrigger::AgentDown(agent),
            message: BusStateMessage {
                content: "down".into(),
                severity: 1.0,
            },
        });
        let mut s = BorrowedStateBusSubscriber::new(&mut rx);
        match s.try_recv() {
            Some(StateMessage::AgentLeft { agent: got }) => assert_eq!(got, agent),
            other => panic!("expected AgentLeft, got {other:?}"),
        }
    }
}
