//! Per-member runner task — closes the gap between formation-level state
//! mutations (which happen on cadence ticks) and async cooperation events
//! (CFPs, peer state, cohesion signals) that arrive between ticks.
//!
//! Per `pure-noodling-biscuit.md` lines 1907–1921: each member should run
//! `agent::tick` against its own slice of formation state. This module is
//! the **runtime** half of that vision — it owns the per-member channel
//! handles and dispatches incoming events to the cooperation crate's
//! step functions.
//!
//! Currently the runner only handles L4 Contract Net bidding (B2). As B6
//! (handoff consumption), B7 (consensus init), B4 (stigmergy sense), and
//! B9 (sacrifice eval) wire through, additional select arms join the loop.

use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;

use springtale_cooperation::agent::AgentContext;
use springtale_cooperation::attention::AttentionBroker;
use springtale_cooperation::cadence::{AgentId, IntentPattern, Tick};
use springtale_cooperation::capability::CapabilityDecl;
use springtale_cooperation::context::FormationContext;
use springtale_cooperation::contract_net::ParticipantHandle;
use springtale_cooperation::contract_net::bid::evaluate;
use springtale_cooperation::contract_net::types::{Bid, CallForProposals};
use springtale_cooperation::momentum::MomentumState;

/// Spawn one runner task per member. Returns the `JoinHandle` so the
/// formation can abort it on `leave`/`Drop`.
///
/// The runner holds:
/// - `ParticipantHandle` (cfp_rx, bid_tx, award_rx, state_rx)
/// - `Arc<AttentionBroker>` for live attention reads
/// - `watch::Receiver<FormationContext>` for live momentum tier reads
/// - the agent's capability list (cloned)
///
/// On each CFP it constructs a synthetic `AgentContext` from the live
/// formation state and calls `evaluate::score`, sending the bid back via
/// `bid_tx` if scoring returned `Some`.
pub fn spawn(
    agent_id: AgentId,
    capabilities: Vec<CapabilityDecl>,
    handle: ParticipantHandle,
    attention_broker: Arc<AttentionBroker>,
    context_rx: watch::Receiver<FormationContext>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        run(agent_id, capabilities, handle, attention_broker, context_rx).await;
    })
}

async fn run(
    agent_id: AgentId,
    capabilities: Vec<CapabilityDecl>,
    mut handle: ParticipantHandle,
    attention_broker: Arc<AttentionBroker>,
    context_rx: watch::Receiver<FormationContext>,
) {
    loop {
        tokio::select! {
            cfp = handle.cfp_rx.recv() => match cfp {
                Ok(cfp) => {
                    on_cfp(
                        &cfp,
                        agent_id,
                        &capabilities,
                        &attention_broker,
                        &context_rx,
                        &handle.bid_tx,
                    );
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(agent = %agent_id.0, skipped = n, "runner cfp_rx lagged");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            },
            else => break,
        }
    }
}

/// Build a synthetic `AgentContext` from live formation state and score
/// the CFP. Returns the bid via `bid_tx` if scoring succeeds.
///
/// The synthetic `Tick` carries the current intent so `score`'s
/// `intent_overlap` factor sees the right data. Sequence is 0 because
/// CFPs arrive between ticks; the value is only used for tracing.
fn on_cfp(
    cfp: &CallForProposals,
    agent_id: AgentId,
    capabilities: &[CapabilityDecl],
    attention_broker: &Arc<AttentionBroker>,
    context_rx: &watch::Receiver<FormationContext>,
    bid_tx: &mpsc::UnboundedSender<Bid>,
) {
    let context = context_rx.borrow().clone();
    let attention = attention_broker.current();
    let momentum = MomentumState {
        tier: context.momentum_tier,
        ..MomentumState::default()
    };
    let tick = synthetic_tick(&context.intent);
    // Runner tasks evaluate CFPs out-of-band; they don't carry the
    // member's per-tick LocalAwareness here. Use a default awareness so
    // CFP scoring sees a neutral peer state — the bidder's hard gate is
    // capability fit, which doesn't need awareness.
    let awareness = springtale_cooperation::awareness::LocalAwareness::default();
    let ctx = AgentContext {
        agent_id,
        tick: &tick,
        formation: &context,
        momentum: &momentum,
        attention: &attention,
        capabilities,
        awareness: &awareness,
    };
    if let Some(utility) = evaluate::score(cfp, &ctx, capabilities) {
        let bid = Bid {
            cfp_id: cfp.id,
            bidder: agent_id,
            utility,
            estimated_completion: cfp.deadline / 2,
            rationale: format!("runner bid (utility = {utility:.3})"),
        };
        if bid_tx.send(bid).is_err() {
            tracing::trace!(agent = %agent_id.0, "bid send failed (initiator gone)");
        }
    }
}

fn synthetic_tick(intent: &IntentPattern) -> Tick {
    Tick {
        sequence: springtale_cooperation::TickId::ZERO,
        timestamp: Instant::now(),
        intent: intent.clone(),
        window: Duration::from_millis(33),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use springtale_cooperation::action::SubTask;
    use springtale_cooperation::contract_net::CfpChannels;
    use uuid::Uuid;

    fn make_subtask(connector: &str) -> SubTask {
        SubTask {
            id: Uuid::new_v4(),
            target_connector: CapabilityDecl::new(connector),
            action_name: "send_message".into(),
            params: serde_json::json!({}),
            priority: 5,
            description: "test".into(),
            depends_on: Vec::new(),
            assigned_to: None,
        }
    }

    #[tokio::test]
    async fn runner_bids_when_capability_matches() {
        let (channels, mut initiator) = CfpChannels::new();
        let agent_id = AgentId::new();
        let caps = vec![CapabilityDecl::new("slack")];
        let attention = Arc::new(AttentionBroker::for_agents(&[agent_id]));
        let (ctx_tx, ctx_rx) = watch::channel(FormationContext::default());

        let handle = spawn(agent_id, caps, channels.participant(), attention, ctx_rx);

        // Give the runner a moment to subscribe.
        tokio::task::yield_now().await;

        let cfp = CallForProposals {
            id: Uuid::new_v4(),
            initiator: AgentId::new(),
            task: make_subtask("slack"),
            deadline: Duration::from_millis(50),
            required_capability: Some(CapabilityDecl::new("slack")),
            scoring_hint: None,
        };
        let cfp_id = cfp.id;
        let _ = channels.cfp_tx.send(cfp);

        let bid = tokio::time::timeout(Duration::from_millis(200), initiator.bid_rx.recv())
            .await
            .expect("bid arrived")
            .expect("bid Some");
        assert_eq!(bid.bidder, agent_id);
        assert_eq!(bid.cfp_id, cfp_id);
        assert!(bid.utility > 0.0);

        handle.abort();
        let _ = ctx_tx; // keep alive until end
    }

    /// Plan §B2 integration test: 3 agents with overlapping capabilities
    /// run a real CFP round through `coordinator::run_round`. Verifies the
    /// initiator-side broadcasts a CFP and the highest-utility bidder wins.
    /// Models the rally-exhaustion takeover path in
    /// `tick_steps/check_cascade.rs::run_takeover_cfp`.
    #[tokio::test]
    async fn cfp_round_picks_best_bidder_among_three() {
        use springtale_cooperation::attention::AttentionEconomy;
        use springtale_cooperation::contract_net::coordinator::{RoundOutcome, run_round};
        use springtale_cooperation::momentum::MomentumTier;

        let (channels, mut initiator) = CfpChannels::new();
        let agent_a = AgentId::new();
        let agent_b = AgentId::new();
        let agent_c = AgentId::new();
        let caps = vec![CapabilityDecl::new("slack")];

        // Agent C is fully free (high free-capacity → highest utility);
        // agent A is loaded; agent B is medium. AttentionEconomy::shift_toward
        // moves attention onto a target; we load A and B so C is the winner.
        let mut econ = AttentionEconomy::new(&[agent_a, agent_b, agent_c]);
        econ.shift_toward(&agent_a, 0.5); // heavy load on A
        econ.shift_toward(&agent_b, 0.2); // medium load on B
        let attention = Arc::new(springtale_cooperation::attention::AttentionBroker::new(
            econ,
        ));

        // L4 unlocks at Hot.
        let ctx = FormationContext {
            momentum_tier: MomentumTier::Hot,
            ..Default::default()
        };
        let (ctx_tx, ctx_rx) = watch::channel(ctx);

        let h_a = spawn(
            agent_a,
            caps.clone(),
            channels.participant(),
            attention.clone(),
            ctx_rx.clone(),
        );
        let h_b = spawn(
            agent_b,
            caps.clone(),
            channels.participant(),
            attention.clone(),
            ctx_rx.clone(),
        );
        let h_c = spawn(
            agent_c,
            caps.clone(),
            channels.participant(),
            attention.clone(),
            ctx_rx.clone(),
        );

        // Give all three runners a moment to subscribe to cfp_rx.
        tokio::task::yield_now().await;
        tokio::time::sleep(Duration::from_millis(10)).await;

        let cfp = CallForProposals {
            id: Uuid::new_v4(),
            initiator: AgentId::new(),
            task: make_subtask("slack"),
            deadline: Duration::from_millis(100),
            required_capability: Some(CapabilityDecl::new("slack")),
            scoring_hint: Some("test takeover".into()),
        };

        let mut initiator_handle = springtale_cooperation::contract_net::InitiatorHandle {
            cfp_tx: channels.cfp_tx.clone(),
            bid_rx: std::mem::replace(&mut initiator.bid_rx, {
                let (_, rx) = tokio::sync::mpsc::unbounded_channel();
                rx
            }),
            award_tx: channels.award_tx.clone(),
            state_tx: channels.state_tx.clone(),
        };
        let outcome = run_round(&mut initiator_handle, cfp, MomentumTier::Hot).await;

        match outcome {
            RoundOutcome::Awarded { award, bids_seen } => {
                assert_eq!(bids_seen, 3, "all three runners should have bid");
                assert_eq!(
                    award.winner, agent_c,
                    "agent C had zero load → highest free-capacity → should win, but {:?} won",
                    award.winner
                );
            }
            other => panic!("expected Awarded, got {other:?}"),
        }

        h_a.abort();
        h_b.abort();
        h_c.abort();
        let _ = ctx_tx; // keep alive
    }

    #[tokio::test]
    async fn runner_skips_when_capability_missing() {
        let (channels, mut initiator) = CfpChannels::new();
        let agent_id = AgentId::new();
        let caps = vec![CapabilityDecl::new("github")];
        let attention = Arc::new(AttentionBroker::for_agents(&[agent_id]));
        let (ctx_tx, ctx_rx) = watch::channel(FormationContext::default());

        let handle = spawn(agent_id, caps, channels.participant(), attention, ctx_rx);
        tokio::task::yield_now().await;

        let cfp = CallForProposals {
            id: Uuid::new_v4(),
            initiator: AgentId::new(),
            task: make_subtask("slack"),
            deadline: Duration::from_millis(50),
            required_capability: Some(CapabilityDecl::new("slack")),
            scoring_hint: None,
        };
        let _ = channels.cfp_tx.send(cfp);

        let result = tokio::time::timeout(Duration::from_millis(50), initiator.bid_rx.recv()).await;
        assert!(result.is_err(), "no bid expected for missing capability");

        handle.abort();
        let _ = ctx_tx;
    }
}
