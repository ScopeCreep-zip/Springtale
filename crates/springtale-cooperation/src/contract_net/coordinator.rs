//! End-to-end Contract Net lifecycle: announce → collect bids → award.
//!
//! This is the *glue* — each phase delegates to a focused module (`cfp/`,
//! `bid/`, `award/`) so this file stays compositional. The L4 authority gate
//! (`authority::require(tier, L4Contested)`) is checked here once, before
//! any channel send, so unauthorized CFPs never touch the bus.

use std::time::Duration;

use crate::authority::{self, Unauthorized};
use crate::contract_net::award::{notify, select};
use crate::contract_net::cfp::announce;
use crate::contract_net::channel::InitiatorHandle;
use crate::contract_net::lifecycle::CfpState;
use crate::contract_net::types::{Award, Bid, CallForProposals};
use crate::layer::LayerId;
use crate::momentum::MomentumTier;

/// Outcome of a single Contract Net round.
#[derive(Debug)]
pub enum RoundOutcome {
    Awarded { award: Award, bids_seen: usize },
    NoBids,
    Unauthorized(Unauthorized),
    AnnounceFailed,
    NotifyFailed,
}

/// Run one Contract Net round against the given initiator handle.
///
/// Phases, each with explicit state transitions for observability:
/// 1. Authority check — L4 unlocks at Hot+.
/// 2. Announce CFP (`CfpState::Announced`).
/// 3. Collect bids for `cfp.deadline` (`CfpState::Collecting`).
/// 4. Pick highest-utility bid via `award::select::highest_utility`.
/// 5. Broadcast Award (`CfpState::Awarded`) or return `NoBids` on empty.
pub async fn run_round(
    handle: &mut InitiatorHandle,
    cfp: CallForProposals,
    tier: MomentumTier,
) -> RoundOutcome {
    if let Err(e) = authority::require(tier, LayerId::L4Contested) {
        return RoundOutcome::Unauthorized(e);
    }

    let cfp_id = cfp.id;
    let deadline = cfp.deadline;

    if announce::announce(&handle.cfp_tx, cfp).is_err() {
        return RoundOutcome::AnnounceFailed;
    }
    let _ = handle.state_tx.send(CfpState::Announced {
        cfp_id,
        started_at: std::time::Instant::now(),
    });

    let bids = collect_bids(&mut handle.bid_rx, cfp_id, deadline).await;
    let _ = handle.state_tx.send(CfpState::Collecting {
        cfp_id,
        bids: bids.clone(),
    });

    let Some(winning) = select::highest_utility(&bids).cloned() else {
        let _ = handle.state_tx.send(CfpState::Expired { cfp_id });
        return RoundOutcome::NoBids;
    };
    let award = Award {
        cfp_id,
        winner: winning.bidder,
        utility: winning.utility,
    };
    let _ = handle.state_tx.send(CfpState::Awarded {
        cfp_id,
        winner: award.winner,
        utility: award.utility,
    });

    if notify::notify(&handle.award_tx, award.clone()).is_err() {
        return RoundOutcome::NotifyFailed;
    }
    RoundOutcome::Awarded {
        award,
        bids_seen: bids.len(),
    }
}

async fn collect_bids(
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<Bid>,
    cfp_id: uuid::Uuid,
    deadline: Duration,
) -> Vec<Bid> {
    use tokio::time::{timeout_at, Instant};
    let stop_at = Instant::now() + deadline;
    let mut bids = Vec::new();
    loop {
        match timeout_at(stop_at, rx.recv()).await {
            Ok(Some(bid)) if bid.cfp_id == cfp_id => bids.push(bid),
            Ok(Some(_)) => continue,
            Ok(None) => break,
            Err(_) => break,
        }
    }
    bids
}
