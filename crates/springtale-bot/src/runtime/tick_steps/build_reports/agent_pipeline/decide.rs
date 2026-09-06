//! Phase 1 of the beat: one member decides against the frozen snapshot.
//!
//! Layer order per plan 1.9 / `cooperation.md` §2: sense, inbox, react,
//! scan, respond_cfp, sacrifice. An inbox hit is the beat's task; a
//! surface reaction is not a claim and never starves the scan.

use std::sync::Arc;

use springtale_cooperation::action::SubTask;
use springtale_cooperation::agent::AgentContext;
use springtale_cooperation::agent::step;
use springtale_cooperation::attention::AttentionEconomy;
use springtale_cooperation::cadence::{ActionDescriptor, AgentId, Tick};
use springtale_cooperation::context::FormationContext;
use springtale_cooperation::contract_net::trait_::Bidder;
use springtale_cooperation::contract_net::types::{Bid, CallForProposals};
use springtale_cooperation::dissemination::BufferedStateSubscriber;
use springtale_cooperation::dissemination::StateMessage;
use springtale_cooperation::momentum::MomentumState;
use springtale_cooperation::sacrifice::SacrificeAction;

use crate::cooperation::blackboard_router::BlackboardRouter;
use crate::cooperation::formation::{Formation, FormationMember};

/// Formation state frozen once per beat so every member decides against
/// the same picture and no member's `&mut` fights another's read.
pub struct Snapshots {
    pub fc: FormationContext,
    pub momentum: MomentumState,
    pub attention: Arc<AttentionEconomy>,
    pub router: Arc<BlackboardRouter>,
    pub surfaces: Arc<dyn springtale_cooperation::stigmergy::SurfaceSubstrate>,
    pub bidder: Arc<dyn Bidder>,
    /// The beat answers the first open CFP (plan 1.9).
    pub open_cfp: Option<CallForProposals>,
    pub rally_tokens: u32,
    pub member_count: usize,
}

impl Snapshots {
    pub fn capture(formation: &Formation) -> Self {
        Self {
            fc: FormationContext {
                intent: formation.intent.clone(),
                momentum_tier: formation.momentum.tier,
                constraints: formation.constraints.clone(),
                guard_mode: formation.constraints.guard_mode,
                operational_count: formation.operational_count(),
                member_count: formation.members.len(),
                paused: formation.paused,
            },
            momentum: formation.momentum.clone(),
            attention: formation.attention_broker.current(),
            router: formation.task_router.clone(),
            surfaces: formation.surfaces.clone(),
            bidder: formation.bidder.clone(),
            open_cfp: formation.open_cfps.first().cloned(),
            rally_tokens: formation.rally.tokens.remaining() as u32,
            member_count: formation.members.len(),
        }
    }
}

/// What one member decided this beat.
pub struct Decision {
    pub agent: AgentId,
    pub tick_action: Option<ActionDescriptor>,
    pub chosen_task: Option<SubTask>,
    pub sacrifice: Option<SacrificeAction>,
    pub bid: Option<Bid>,
}

pub async fn run(
    member: &mut FormationMember,
    tick: &Tick,
    s: &Snapshots,
    drained: Option<&Vec<StateMessage>>,
) -> Decision {
    let mut decision = Decision {
        agent: member.agent_id,
        tick_action: None,
        chosen_task: None,
        sacrifice: None,
        bid: None,
    };
    let mut needs_scan = true;

    // Borrow scoping: react needs `&mut member.awareness`, while sense,
    // inbox, scan and respond_cfp read it through `AgentContext`. The ctx
    // is built twice (pre-react, post-react) so the immutable borrow is
    // released before react mutates.
    {
        let ctx = AgentContext {
            agent_id: member.agent_id,
            tick,
            formation: &s.fc,
            momentum: &s.momentum,
            attention: &s.attention,
            capabilities: &member.capabilities,
            awareness: &member.awareness,
        };
        if let Some(r) = step::sense::run(s.surfaces.as_ref(), &member.awareness, &ctx) {
            // A surface reaction is not a task claim: the scan still runs
            // (plan 1.9 / finding 40). Only an inbox hit skips it.
            decision.tick_action = r.action;
            decision.chosen_task = r.task_claimed;
        } else if let Some(r) = step::inbox::run(s.router.as_ref(), &ctx).await {
            decision.tick_action = r.action;
            decision.chosen_task = r.task_claimed;
            needs_scan = false;
        }
    }

    // React folds pre-drained bus state into awareness so the scan and
    // the bid see fresh peer state. No tick action.
    if let Some(msgs) = drained {
        let mut buf = BufferedStateSubscriber::new(msgs.clone());
        step::react::run(&mut buf, &mut member.awareness, s.momentum.tier);
    }
    let ctx = AgentContext {
        agent_id: member.agent_id,
        tick,
        formation: &s.fc,
        momentum: &s.momentum,
        attention: &s.attention,
        capabilities: &member.capabilities,
        awareness: &member.awareness,
    };
    if needs_scan && let Some(r) = step::scan::run(s.router.as_ref(), &ctx).await {
        decision.tick_action = r.action;
        decision.chosen_task = r.task_claimed;
    }
    // L4 contract net inside the beat (plan 1.9): bid on the open CFP.
    if let Some(cfp) = s.open_cfp.as_ref() {
        decision.bid = step::respond_cfp::run(s.bidder.as_ref(), cfp, &ctx).await;
    }
    // B9 final consideration — at Hot+ tier the agent checks whether
    // yielding to a more-loaded peer is the higher-utility play; a yield
    // drops the chosen task and reports a yield-shaped descriptor.
    if needs_scan {
        decision.sacrifice = step::sacrifice::run(&ctx, s.rally_tokens, s.member_count, &[]);
        if decision.sacrifice.is_some() {
            decision.chosen_task = None;
        }
    }
    decision
}
