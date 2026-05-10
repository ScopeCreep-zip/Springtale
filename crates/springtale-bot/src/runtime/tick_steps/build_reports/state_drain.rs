//! Pre-drain every member's bus state subscription into a HashMap before
//! the per-member loop runs. The `agent::step::react` step then folds
//! each member's pre-drained messages into their `LocalAwareness` from
//! a `BufferedStateSubscriber`.
//!
//! Done up front because `formation.member_subs` is a `std::sync::Mutex`
//! field on `Formation` and the per-member loop holds
//! `&mut formation.members` — split-borrowing the two through async
//! await boundaries inside the loop is awkward, so we collect everything
//! once and apply per member.

use std::collections::HashMap;

use springtale_cooperation::cadence::AgentId;
use springtale_cooperation::dissemination::trait_::StateSubscriber;
use springtale_cooperation::dissemination::{BorrowedStateBusSubscriber, StateMessage};

use crate::cooperation::formation::Formation;

/// Returns `agent_id → Vec<StateMessage>` for every member that had at
/// least one queued message. Lagged broadcasts (channel overflow) are
/// silently skipped per `COOPERATION.md §19` lossy-state semantics.
pub fn run(formation: &Formation) -> HashMap<AgentId, Vec<StateMessage>> {
    let mut out: HashMap<AgentId, Vec<StateMessage>> = HashMap::new();
    let Ok(mut subs) = formation.member_subs.lock() else {
        return out;
    };
    for (agent_id, sub) in subs.iter_mut() {
        let mut bsub = BorrowedStateBusSubscriber::new(&mut sub.state_rx);
        let mut msgs = Vec::new();
        while let Some(m) = bsub.try_recv() {
            msgs.push(m);
        }
        if !msgs.is_empty() {
            out.insert(*agent_id, msgs);
        }
    }
    out
}
