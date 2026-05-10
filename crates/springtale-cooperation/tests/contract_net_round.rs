//! L4 Contract Net integration test.
//!
//! Scenario: 3 agents, 1 CFP, utility-scored bids. Initiator announces; two
//! capable bidders submit; one non-capable bidder abstains. Coordinator
//! collects, picks the highest-utility bid, broadcasts the award.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use springtale_cooperation::action::SubTask;
use springtale_cooperation::agent::AgentContext;
use springtale_cooperation::attention::AttentionEconomy;
use springtale_cooperation::cadence::{AgentId, IntentPattern, Tick};
use springtale_cooperation::context::FormationContext;
use springtale_cooperation::contract_net::bid::evaluate::UtilityBidder;
use springtale_cooperation::contract_net::cfp::descriptor;
use springtale_cooperation::contract_net::trait_::Bidder;
use springtale_cooperation::contract_net::{CfpChannels, RoundOutcome};
use springtale_cooperation::momentum::{MomentumState, MomentumTier};
use springtale_cooperation::types::FormationConstraints;

fn make_subtask(connector: &str) -> SubTask {
    SubTask {
        id: uuid::Uuid::new_v4(),
        target_connector: springtale_cooperation::capability::CapabilityDecl::new(connector),
        action_name: "process".to_owned(),
        params: serde_json::json!({}),
        priority: 2,
        assigned_to: None,
        description: "contested task".to_owned(),
    }
}

fn make_tick() -> Tick {
    Tick {
        sequence: 1,
        timestamp: std::time::Instant::now(),
        intent: IntentPattern::Execute { plan_id: None },
        window: Duration::from_millis(33),
    }
}

fn make_formation() -> FormationContext {
    FormationContext {
        intent: IntentPattern::Execute { plan_id: None },
        momentum_tier: MomentumTier::Hot,
        constraints: FormationConstraints::default(),
        guard_mode: false,
        operational_count: 3,
        member_count: 3,
        paused: false,
    }
}

fn make_momentum() -> MomentumState {
    MomentumState {
        tier: MomentumTier::Hot,
        ..Default::default()
    }
}

async fn run_bidder(
    mut cfp_rx: tokio::sync::broadcast::Receiver<
        springtale_cooperation::contract_net::types::CallForProposals,
    >,
    bid_tx: tokio::sync::mpsc::UnboundedSender<springtale_cooperation::contract_net::types::Bid>,
    agent: AgentId,
    capabilities: Vec<springtale_cooperation::capability::CapabilityDecl>,
    attention: AttentionEconomy,
) {
    let Ok(cfp) = cfp_rx.recv().await else { return };
    let tick = make_tick();
    let formation = make_formation();
    let momentum = make_momentum();
    let bidder = UtilityBidder::new(&capabilities);
    let aw = springtale_cooperation::awareness::LocalAwareness::default();
    let ctx = AgentContext {
        agent_id: agent,
        tick: &tick,
        formation: &formation,
        momentum: &momentum,
        attention: &attention,
        capabilities: &capabilities,
        awareness: &aw,
    };
    if let Some(bid) = bidder.evaluate(&cfp, &ctx).await {
        let _ = bid_tx.send(bid);
    }
}

#[tokio::test]
async fn three_bidders_one_winner() {
    let initiator = AgentId::new();
    let bidder_a = AgentId::new();
    let bidder_b = AgentId::new();
    let bidder_c = AgentId::new();

    let (channels, mut handle) = CfpChannels::new();
    let a = channels.participant();
    let b = channels.participant();
    let c = channels.participant();

    let cfp = descriptor::for_task(
        initiator,
        make_subtask("github"),
        Duration::from_millis(120),
        Some(springtale_cooperation::capability::CapabilityDecl::new("github")),
    );

    let mut base_attention = AttentionEconomy::new(&[bidder_a, bidder_b, bidder_c]);
    base_attention.shift_toward(&bidder_b, 0.3); // B is busier → lower bid.

    let task_a = tokio::spawn(run_bidder(
        a.cfp_rx,
        a.bid_tx,
        bidder_a,
        vec!["github".into()],
        base_attention.clone(),
    ));
    let task_b = tokio::spawn(run_bidder(
        b.cfp_rx,
        b.bid_tx,
        bidder_b,
        vec!["github".into()],
        base_attention.clone(),
    ));
    let task_c = tokio::spawn(run_bidder(
        c.cfp_rx,
        c.bid_tx,
        bidder_c,
        vec!["slack".into()],
        base_attention.clone(),
    ));

    let outcome =
        springtale_cooperation::contract_net::run_round(&mut handle, cfp, MomentumTier::Hot).await;

    let _ = tokio::time::timeout(Duration::from_millis(200), task_a).await;
    let _ = tokio::time::timeout(Duration::from_millis(200), task_b).await;
    let _ = tokio::time::timeout(Duration::from_millis(200), task_c).await;

    match outcome {
        RoundOutcome::Awarded { award, bids_seen } => {
            assert_eq!(bids_seen, 2, "C abstains, A+B bid");
            assert_eq!(
                award.winner, bidder_a,
                "A has lower attention load, so it wins"
            );
        }
        other => panic!("expected Awarded, got {other:?}"),
    }
}

#[tokio::test]
async fn cold_formation_blocks_cfp() {
    let initiator = AgentId::new();
    let (_channels, mut handle) = CfpChannels::new();
    let cfp = descriptor::for_task(
        initiator,
        make_subtask("github"),
        Duration::from_millis(20),
        Some(springtale_cooperation::capability::CapabilityDecl::new("github")),
    );
    let outcome =
        springtale_cooperation::contract_net::run_round(&mut handle, cfp, MomentumTier::Cold).await;
    assert!(matches!(outcome, RoundOutcome::Unauthorized(_)));
}

#[tokio::test]
async fn no_bids_returns_no_bids() {
    let initiator = AgentId::new();
    let (channels, mut handle) = CfpChannels::new();
    // A subscriber must exist — otherwise the CFP broadcast has nowhere to
    // go and the round correctly reports AnnounceFailed instead of NoBids.
    // The subscriber here is a silent participant: it receives the CFP but
    // never bids.
    let _silent = channels.participant();

    let cfp = descriptor::for_task(
        initiator,
        make_subtask("github"),
        Duration::from_millis(20),
        Some(springtale_cooperation::capability::CapabilityDecl::new("github")),
    );
    let outcome =
        springtale_cooperation::contract_net::run_round(&mut handle, cfp, MomentumTier::Hot).await;
    assert!(matches!(outcome, RoundOutcome::NoBids), "got {outcome:?}");
}
