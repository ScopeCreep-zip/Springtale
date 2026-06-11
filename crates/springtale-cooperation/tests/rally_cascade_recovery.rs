//! Rally cascade + token-budget recovery integration (Phase-7 audit C).
//!
//! Exercises the full Monster-Hunter "cart" loop:
//!
//! 1. Build a `FormationRally` with N token cart budget.
//! 2. Drive N self-rallies from cascade failures — every rally
//!    succeeds, tokens decrement monotonically, broadcast events
//!    record the cost.
//! 3. The very last consume() closes the latch (zero tokens left).
//! 4. One more rally request escalates rather than consuming, with
//!    no negative token count and no in-flight event loss.
//! 5. A persistence-hydrated rally that reports fewer remaining
//!    tokens than its max budget exposes the on-disk state, and the
//!    next rally consumes from the restored pool — not from a
//!    phantom re-budget.
//!
//! Validates the rally invariants spec §15: tokens are limited like
//! MH carts, exhaust → escalate, no operation lost, events are
//! recoverable via the broadcast channel.

#![allow(clippy::unwrap_used)]

use springtale_cooperation::attention::AttentionBroker;
use springtale_cooperation::cadence::AgentId;
use springtale_cooperation::momentum::MomentumState;
use springtale_cooperation::rally::cascade::attempt_self_rally;
use springtale_cooperation::rally::{FormationRally, RallyEvent, RallyResult};

const TOKEN_BUDGET: usize = 3;
const EVENT_CAP: usize = 16;

#[test]
fn cascade_consumes_tokens_then_escalates() {
    // Four-agent formation: `a` is the failing agent every rally
    // redistributes attention away from; `b` is the one who triggers
    // the post-exhaustion escalation; `c` and `d` are the healthy
    // peers attention flows toward. AttentionBroker requires every
    // participant up front so release() against `a` redistributes
    // against the right neighbour set.
    let a = AgentId::new();
    let b = AgentId::new();
    let c = AgentId::new();
    let d = AgentId::new();
    let agents = [a, b, c, d];

    let rally = FormationRally::new(TOKEN_BUDGET, EVENT_CAP);
    let attention = AttentionBroker::for_agents(&agents);
    let mut momentum = MomentumState::default();

    // Sanity check: every agent is registered with the broker before
    // we begin so attention.release() has a valid neighbour set to
    // redistribute load to. for_agents() should populate the
    // economy's HashMap with one entry per agent; the test fails
    // fast here if registration is ever silently dropped.
    {
        let economy = attention.current();
        for agent in agents {
            assert!(
                economy.agents().contains_key(&agent),
                "AttentionBroker missing agent {agent:?} after for_agents()"
            );
        }
    }

    // Subscribe before the first rally so we observe every event.
    let mut observer = rally.subscribe();

    // ── Force `failing_agent = a` through TOKEN_BUDGET successive
    //    rallies. Each must succeed and decrement the token count
    //    monotonically. ──
    let expected_remaining: Vec<u32> = (0..TOKEN_BUDGET).rev().map(|n| n as u32).collect();
    let mut seen_remaining = Vec::new();
    for _ in 0..TOKEN_BUDGET {
        let result = attempt_self_rally(&rally, &attention, &mut momentum, a);
        match result {
            RallyResult::StabilizedWithCost { tokens_remaining } => {
                seen_remaining.push(tokens_remaining);
            }
            other => panic!("expected StabilizedWithCost, got {other:?}"),
        }
    }
    assert_eq!(
        seen_remaining, expected_remaining,
        "tokens must decrement monotonically across consecutive rallies"
    );

    // ── The next rally finds zero tokens AND the latch closed; the
    //    cascade detector observes EscalateToOrchestrator. ──
    assert_eq!(rally.tokens.remaining(), 0);
    assert!(!rally.tokens.can_rally());

    let escalation = attempt_self_rally(&rally, &attention, &mut momentum, b);
    match escalation {
        RallyResult::EscalateToOrchestrator { reason } => {
            assert!(reason.contains("exhausted"), "got reason: {reason}");
        }
        other => panic!("expected EscalateToOrchestrator, got {other:?}"),
    }

    // ── Drain broadcast events and assert we have:
    //    - 3 × AttentionRedistributed (one per rally)
    //    - 3 × TokenConsumed (with monotonic remaining)
    //    - 1 × Escalated (the post-exhaustion rally)
    // The bus may include other event kinds; we count by variant. ──
    let mut redistributed = 0usize;
    let mut consumed = 0usize;
    let mut consumed_remaining = Vec::new();
    let mut escalated = 0usize;
    while let Ok(ev) = observer.try_recv() {
        match ev {
            RallyEvent::AttentionRedistributed { .. } => redistributed += 1,
            RallyEvent::TokenConsumed { remaining } => {
                consumed += 1;
                consumed_remaining.push(remaining);
            }
            RallyEvent::Escalated { .. } => escalated += 1,
            _ => {}
        }
    }
    assert_eq!(redistributed, TOKEN_BUDGET);
    assert_eq!(consumed, TOKEN_BUDGET);
    assert_eq!(consumed_remaining, expected_remaining);
    assert_eq!(escalated, 1);

    // ── No drift: the count of broadcast events accounts for every
    //    rally + escalation; nothing was lost. ──
    let total_lifecycle_events = redistributed + consumed + escalated;
    assert_eq!(total_lifecycle_events, TOKEN_BUDGET * 2 + 1);

    // ── Post-rally invariant: attention.release() against agent `a`
    //    ran TOKEN_BUDGET times. The broker tolerates re-release
    //    without panic and the healthy peers' load tracking remains
    //    queryable. Total attention is conserved across the
    //    economy — every release of `a`'s load is absorbed by the
    //    remaining agents, and the sum stays in the [0, agent_count]
    //    band the AttentionEconomy enforces. ──
    let economy = attention.current();
    let a_load = economy.load(&a);
    let total: f32 = agents.iter().map(|id| economy.load(id)).sum();
    assert!(
        a_load < economy.load(&c),
        "failing agent {a:?} should have lower load than healthy peer {c:?} after cascade"
    );
    assert!(
        a_load < economy.load(&d),
        "failing agent {a:?} should have lower load than healthy peer {d:?} after cascade"
    );
    assert!(
        total.is_finite() && total >= 0.0,
        "attention economy total drifted out of range: {total}"
    );
}

#[test]
fn restore_tokens_reflects_persisted_state() {
    let a = AgentId::new();
    let rally = FormationRally::new(TOKEN_BUDGET, EVENT_CAP);
    // Pretend disk says "1 token remaining" — restore consumes
    // (TOKEN_BUDGET - 1) = 2 permits up front.
    rally.restore_tokens(1);
    assert_eq!(rally.tokens.remaining(), 1);

    let attention = AttentionBroker::for_agents(&[a]);
    let mut momentum = MomentumState::default();
    let first = attempt_self_rally(&rally, &attention, &mut momentum, a);
    assert!(matches!(
        first,
        RallyResult::StabilizedWithCost {
            tokens_remaining: 0,
        }
    ));

    // Latch closed — restored exhausted state stays exhausted.
    let second = attempt_self_rally(&rally, &attention, &mut momentum, a);
    assert!(matches!(second, RallyResult::EscalateToOrchestrator { .. }));
}
