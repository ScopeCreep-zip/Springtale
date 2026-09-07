//! Cascade detection and self-rally algorithm.
//!
//! Per COOPERATION.pdf §15:
//! §15.1 Cascade Detection: Agent A fails → neighbors see it →
//!   their morale drops → cascade risk.
//! §15.2 Formation Self-Rally (before escalating to orchestrator):
//!   1. Redistribute attention (§9) away from struggling agent
//!   2. Transform roles (§14) for failed agent
//!   3. Reduce momentum tier to match reduced coherence
//!   4. Consume rally token (limited, like Monster Hunter carts)

use std::collections::{HashMap, HashSet};

use crate::action_state::ActionState;
use crate::attention::AttentionBroker;
use crate::awareness::{LocalAwareness, SHATTERED_MORALE};
use crate::cadence::AgentId;
use crate::tick_processor::FormationTickResult;

use super::{FormationRally, RallyEvent, RallyFailure, RallyResult};

/// How severe the cascade risk is.
#[derive(Debug, Clone, PartialEq)]
pub enum CascadeRisk {
    /// One agent failing, neighbors still healthy.
    Low,
    /// Multiple agents with low morale, cascade likely.
    High,
    /// Formation-wide failure imminent.
    Critical,
}

/// Detect cascade risk from awareness state and tick results.
///
/// Per Total War: routing cascade occurs when multiple nearby units
/// have low morale simultaneously. A single routing unit can cause
/// neighbors to break, which causes THEIR neighbors to break.
///
/// Thresholds per spec §15.1:
/// - Low: 1 agent failed, neighbors morale > 0.3
/// - High: 2+ agents with morale < 0.3
/// - Critical: >50% of formation with morale < 0.3
pub fn detect_cascade(
    awareness_map: &HashMap<AgentId, &LocalAwareness>,
    tick_result: &FormationTickResult,
) -> Option<CascadeRisk> {
    if tick_result.all_succeeded {
        return None;
    }

    let total = awareness_map.len();
    if total == 0 {
        return None;
    }

    let low_morale_count = awareness_map
        .values()
        .filter(|a| a.local_morale() < 0.3)
        .count();

    let failed_count = tick_result
        .reports
        .iter()
        .filter(|r| r.intent_alignment <= 0.5)
        .count();

    if low_morale_count > total / 2 {
        Some(CascadeRisk::Critical)
    } else if low_morale_count >= 2 || (failed_count >= 2 && low_morale_count >= 1) {
        Some(CascadeRisk::High)
    } else if failed_count >= 1 {
        Some(CascadeRisk::Low)
    } else {
        None
    }
}

/// Pick the member a rally token should be spent on.
///
/// Per Total War (spec §15.2): a general rallies the unit that can still
/// answer him. A shattered unit is past rallying and a dead one cannot
/// hear it, so a token spent on either is a token wasted. Of the members
/// that failed this beat we take the one with the LOWEST morale that is
/// still above [`SHATTERED_MORALE`] — the closest to breaking that can
/// still be pulled back.
///
/// `candidates` holds only members the caller considers alive and
/// operational (the bot's `check_cascade` builds it from
/// `FormationMember::is_operational`, which excludes `Incapacitated` and
/// `Dead`), so absence from the map is the "cannot respond" answer.
/// `None` means no token should be spent at all.
///
/// `failing` is walked in order so ties resolve deterministically.
pub fn select_rally_target(
    candidates: &HashMap<AgentId, &LocalAwareness>,
    failing: &[AgentId],
) -> Option<AgentId> {
    let mut best: Option<(AgentId, f32)> = None;
    for agent in failing {
        let Some(awareness) = candidates.get(agent) else {
            continue; // incapacitated, dead, or gone: cannot respond
        };
        let morale = awareness.local_morale();
        if morale <= SHATTERED_MORALE {
            continue; // shattered: past rallying (Total War §15.2)
        }
        if best.is_none_or(|(_, lowest)| morale < lowest) {
            best = Some((*agent, morale));
        }
    }
    best.map(|(agent, _)| agent)
}

/// Did a member we spent a rally token on come back?
///
/// `RallyResult::Recovered` is the answer to a rally on a LATER beat:
/// the token bought the member a chance and it finished work with it.
/// `rallied` is the set of members with a token spent on them since the
/// last recovery; `Some(Recovered)` means the caller should clear it.
pub fn recovered(rallied: &HashSet<AgentId>, result: &FormationTickResult) -> Option<RallyResult> {
    if rallied.is_empty() {
        return None;
    }
    result
        .reports
        .iter()
        .any(|r| {
            rallied.contains(&r.agent_id)
                && matches!(r.state, ActionState::Success)
                && r.intent_alignment > 0.5
        })
        .then_some(RallyResult::Recovered)
}

/// Attempt formation self-rally before escalating to orchestrator.
///
/// Per §15.2 (Monster Hunter cart system):
/// 1. Redistribute attention away from failing agent
/// 2. Consume a rally token (the tick's own momentum step already
///    recorded the failure — see the comment in the body)
/// 3. If no tokens left → escalate
///
/// Takes `&FormationRally` (not `&mut`): the token pool is backed by
/// `Arc<Semaphore>` (interior-mutable), the event channel is a
/// broadcast sender. No `&mut` contention on the event-loop hot path.
pub fn attempt_self_rally(
    rally: &FormationRally,
    attention: &AttentionBroker,
    failing_agent: AgentId,
) -> RallyResult {
    if !rally.tokens.can_rally() {
        let reason = "rally tokens exhausted".to_owned();
        let _ = rally.events.send(RallyEvent::Escalated {
            reason: reason.clone(),
        });
        return RallyResult::EscalateToOrchestrator { reason };
    }

    // 1. Redistribute attention away from failing agent
    //    Other agents absorb the load (Army of Two aggro shift)
    attention.release(failing_agent, 0.2);
    let _ = rally.events.send(RallyEvent::AttentionRedistributed {
        from: failing_agent,
    });

    // The formation's lost coherence is NOT recorded here. The beat that
    // produced this cascade already ran `update_momentum`, which recorded
    // the same failed tick; recording it again demoted the formation twice
    // for one bad beat (and `supervision`/`handle_command` rallies would
    // have invented a failure that no tick reported). Momentum is the
    // momentum step's to own — the rally's cost is the token.

    // 2. Consume rally token. `consume()` fails only if something closed
    //    the semaphore between the `can_rally` check and here; treat as
    //    escalation.
    match rally.tokens.consume() {
        Ok(()) => {
            let remaining = rally.tokens.remaining() as u32;
            let _ = rally.events.send(RallyEvent::TokenConsumed { remaining });
            // Last cart: arm the escalation latch so the next failure
            // short-circuits to orchestrator intervention.
            if remaining == 0 {
                rally.tokens.close();
            }
            RallyResult::StabilizedWithCost {
                tokens_remaining: remaining,
            }
        }
        Err(RallyFailure::NoTokensLeft | RallyFailure::Closed) => {
            let reason = "rally tokens exhausted".to_owned();
            let _ = rally.events.send(RallyEvent::Escalated {
                reason: reason.clone(),
            });
            RallyResult::EscalateToOrchestrator { reason }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::cadence::TickReport;
    use crate::momentum::MomentumState;
    use std::time::Duration;

    fn make_report(agent: AgentId, alignment: f32) -> TickReport {
        make_stated(agent, alignment, ActionState::Success)
    }

    fn make_stated(agent: AgentId, alignment: f32, state: ActionState) -> TickReport {
        TickReport {
            agent_id: agent,
            tick_sequence: crate::tick::TickId(1),
            action_taken: Some(crate::cadence::ActionDescriptor {
                kind: "work".to_owned(),
                target: None,
                payload_hash: 0,
            }),
            latency: Duration::from_millis(5),
            intent_alignment: alignment,
            interference_with: vec![],
            surface_reaction: None,
            state,
        }
    }

    fn make_awareness_low_morale() -> LocalAwareness {
        // An awareness with no healthy neighbors = low morale
        let mut aw = LocalAwareness::default();
        let neighbor = AgentId::new();
        aw.update_neighbor(crate::awareness::NeighborSnapshot {
            agent_id: neighbor,
            health: crate::types::AgentHealth::Incapacitated,
            role: crate::awareness::RoleSignature::General,
            fuel_remaining_pct: 0.0,
            last_action_success: false,
            attention_load: 0.0,
            liveness: crate::supervision::Liveness::Alive,
            last_updated: std::time::Instant::now(),
        });
        // Morale is now stateful/lerped; snap it to the (low) target so
        // `local_morale()` reflects the distressed neighbor for this unit test.
        aw.morale = aw.morale_target();
        aw
    }

    #[test]
    fn test_no_cascade_on_success() {
        let a = AgentId::new();
        let awareness_map: HashMap<AgentId, &LocalAwareness> = HashMap::new();
        let result = FormationTickResult {
            reports: vec![make_report(a, 0.9)],
            interferences: vec![],
            all_succeeded: true,
        };
        assert!(detect_cascade(&awareness_map, &result).is_none());
    }

    #[test]
    fn test_low_cascade_single_failure() {
        let a = AgentId::new();
        let b = AgentId::new();
        let aw_a = LocalAwareness::default();
        let aw_b = LocalAwareness::default();
        let mut map = HashMap::new();
        map.insert(a, &aw_a);
        map.insert(b, &aw_b);

        let result = FormationTickResult {
            reports: vec![make_report(a, 0.3), make_report(b, 0.9)],
            interferences: vec![],
            all_succeeded: false,
        };

        let risk = detect_cascade(&map, &result);
        assert_eq!(risk, Some(CascadeRisk::Low));
    }

    #[test]
    fn test_high_cascade_multiple_low_morale() {
        let a = AgentId::new();
        let b = AgentId::new();
        let c = AgentId::new();
        let d = AgentId::new();
        let aw_a = make_awareness_low_morale();
        let aw_b = make_awareness_low_morale();
        let aw_c = LocalAwareness::default();
        let aw_d = LocalAwareness::default();
        let mut map = HashMap::new();
        map.insert(a, &aw_a);
        map.insert(b, &aw_b);
        map.insert(c, &aw_c);
        map.insert(d, &aw_d);

        let result = FormationTickResult {
            reports: vec![
                make_report(a, 0.3),
                make_report(b, 0.3),
                make_report(c, 0.9),
                make_report(d, 0.9),
            ],
            interferences: vec![],
            all_succeeded: false,
        };

        // 2 out of 4 with low morale = 50% = High (not Critical, >50% needed)
        let risk = detect_cascade(&map, &result);
        assert_eq!(risk, Some(CascadeRisk::High));
    }

    #[test]
    fn test_self_rally_consumes_token() {
        let a = AgentId::new();
        let b = AgentId::new();
        let rally = FormationRally::new(3, 8);
        let attention = AttentionBroker::for_agents(&[a, b]);
        let mut momentum = MomentumState::default();

        for _ in 0..5 {
            momentum.record_success();
        }

        let before = momentum.tier;
        let successes = momentum.consecutive_successes;
        let result = attempt_self_rally(&rally, &attention, a);
        assert!(matches!(
            result,
            RallyResult::StabilizedWithCost {
                tokens_remaining: 2
            }
        ));
        assert_eq!(rally.tokens.remaining(), 2);
        // Fix 2 — the rally does not double-count the failure. The beat's
        // momentum step already recorded it; the rally's cost is the token.
        assert_eq!(momentum.tier, before);
        assert_eq!(momentum.consecutive_successes, successes);
    }

    #[test]
    fn test_self_rally_exhausted_escalates() {
        let a = AgentId::new();
        let rally = FormationRally::new(3, 8);
        // Consume all tokens up front to simulate exhausted state.
        rally.tokens.consume().unwrap();
        rally.tokens.consume().unwrap();
        rally.tokens.consume().unwrap();
        let attention = AttentionBroker::for_agents(&[a]);
        let momentum = MomentumState::default();

        let result = attempt_self_rally(&rally, &attention, a);
        assert!(matches!(result, RallyResult::EscalateToOrchestrator { .. }));
        assert_eq!(momentum.consecutive_successes, 0);
    }

    /// Fix 2 — the token goes to the member that can still respond: the
    /// lowest morale ABOVE the shattered floor. A shattered member and a
    /// member missing from the map (dead/incapacitated) are skipped even
    /// though they failed first.
    #[test]
    fn test_select_rally_target_is_lowest_morale_above_shattered() {
        let dead = AgentId::new();
        let shattered = AgentId::new();
        let wavering = AgentId::new();
        let steady = AgentId::new();

        let aw_shattered = LocalAwareness {
            morale: SHATTERED_MORALE - 0.01,
            ..Default::default()
        };
        let aw_wavering = LocalAwareness {
            morale: 0.25,
            ..Default::default()
        };
        let aw_steady = LocalAwareness {
            morale: 0.8,
            ..Default::default()
        };

        // `dead` is absent: `check_cascade` builds this map from
        // operational members only.
        let mut map: HashMap<AgentId, &LocalAwareness> = HashMap::new();
        map.insert(shattered, &aw_shattered);
        map.insert(wavering, &aw_wavering);
        map.insert(steady, &aw_steady);

        let failing = [dead, shattered, wavering, steady];
        assert_eq!(select_rally_target(&map, &failing), Some(wavering));
    }

    /// No failing member can respond → no target, so no token is spent.
    #[test]
    fn test_select_rally_target_none_when_nobody_can_respond() {
        let dead = AgentId::new();
        let shattered = AgentId::new();
        let aw = LocalAwareness {
            morale: 0.0,
            ..Default::default()
        };
        let mut map: HashMap<AgentId, &LocalAwareness> = HashMap::new();
        map.insert(shattered, &aw);
        assert_eq!(select_rally_target(&map, &[dead, shattered]), None);
    }

    /// Fix 3 — `Recovered` is emitted when a rallied member finishes work
    /// on a later beat, and only then.
    #[test]
    fn test_recovered_only_when_a_rallied_member_completes_work() {
        let rallied_agent = AgentId::new();
        let other = AgentId::new();
        let mut rallied = HashSet::new();
        rallied.insert(rallied_agent);

        let still_trying = FormationTickResult {
            reports: vec![make_stated(rallied_agent, 0.8, ActionState::Requested)],
            interferences: vec![],
            all_succeeded: false,
        };
        assert!(recovered(&rallied, &still_trying).is_none());

        let someone_else = FormationTickResult {
            reports: vec![make_report(other, 1.0)],
            interferences: vec![],
            all_succeeded: true,
        };
        assert!(recovered(&rallied, &someone_else).is_none());

        let came_back = FormationTickResult {
            reports: vec![make_report(rallied_agent, 1.0)],
            interferences: vec![],
            all_succeeded: true,
        };
        assert!(matches!(
            recovered(&rallied, &came_back),
            Some(RallyResult::Recovered)
        ));
        assert!(recovered(&HashSet::new(), &came_back).is_none());
    }
}
