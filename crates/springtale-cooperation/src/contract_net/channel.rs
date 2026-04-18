//! L4 channel matrix — broadcast CFP + bid return + broadcast Award.
//!
//! Kept out of the generic `FormationBus` so L4 can be enabled per-formation
//! without forcing every formation to carry Contract Net state. The initiator
//! owns the `bid_rx` end; bidders hold clones of `cfp_rx` + `bid_tx`; every
//! participant subscribes to `award_rx` for winner/reject notifications.

use tokio::sync::{broadcast, mpsc, watch};

use super::lifecycle::CfpState;
use super::types::{Award, Bid, CallForProposals};

/// Transmit side of the Contract Net channels — held by the formation's L4
/// coordinator and distributed to participants via [`CfpChannels::participant`].
pub struct CfpChannels {
    pub cfp_tx: broadcast::Sender<CallForProposals>,
    pub bid_tx: mpsc::UnboundedSender<Bid>,
    pub award_tx: broadcast::Sender<Award>,
    pub state_tx: watch::Sender<CfpState>,
}

/// Receiver set given to a participating agent.
pub struct ParticipantHandle {
    pub cfp_rx: broadcast::Receiver<CallForProposals>,
    pub bid_tx: mpsc::UnboundedSender<Bid>,
    pub award_rx: broadcast::Receiver<Award>,
    /// Observable lifecycle state — per §8/§12, agents can watch the CFP
    /// round's progress so the awareness system can gossip transitions.
    pub state_rx: watch::Receiver<CfpState>,
}

/// Receiver set retained by the initiator — owns the bid mpsc end.
pub struct InitiatorHandle {
    pub cfp_tx: broadcast::Sender<CallForProposals>,
    pub bid_rx: mpsc::UnboundedReceiver<Bid>,
    pub award_tx: broadcast::Sender<Award>,
    /// Lifecycle state channel — the coordinator writes transitions here;
    /// participants and observers subscribe via their `state_rx`.
    pub state_tx: watch::Sender<CfpState>,
}

impl CfpChannels {
    /// Create a fresh channel set. The initiator handle contains the unique
    /// bid receiver; participant handles are cloned from the broadcast
    /// senders.
    pub fn new() -> (Self, InitiatorHandle) {
        let (cfp_tx, _cfp_rx_seed) = broadcast::channel(32);
        let (bid_tx, bid_rx) = mpsc::unbounded_channel();
        let (award_tx, _award_rx_seed) = broadcast::channel(32);
        let (state_tx, _state_rx_seed) = watch::channel(CfpState::Expired {
            cfp_id: uuid::Uuid::nil(),
        });
        let channels = Self {
            cfp_tx: cfp_tx.clone(),
            bid_tx: bid_tx.clone(),
            award_tx: award_tx.clone(),
            state_tx: state_tx.clone(),
        };
        let initiator = InitiatorHandle {
            cfp_tx,
            bid_rx,
            award_tx,
            state_tx,
        };
        (channels, initiator)
    }

    /// Hand a participant its receiver set.
    pub fn participant(&self) -> ParticipantHandle {
        ParticipantHandle {
            cfp_rx: self.cfp_tx.subscribe(),
            bid_tx: self.bid_tx.clone(),
            award_rx: self.award_tx.subscribe(),
            state_rx: self.state_tx.subscribe(),
        }
    }
}
