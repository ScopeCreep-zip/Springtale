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

use std::collections::HashMap;

use crate::attention::AttentionBroker;
use crate::awareness::LocalAwareness;
use crate::cadence::AgentId;
use crate::momentum::MomentumState;
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

/// Attempt formation self-rally before escalating to orchestrator.
///
/// Per §15.2 (Monster Hunter cart system):
/// 1. Redistribute attention away from failing agent
/// 2. Reduce momentum to match reduced coherence
/// 3. Consume a rally token
/// 4. If no tokens left → escalate
///
/// Takes `&FormationRally` (not `&mut`): the token pool is backed by
/// `Arc<Semaphore>` (interior-mutable), the event channel is a
/// broadcast sender. No `&mut` contention on the event-loop hot path.
pub fn attempt_self_rally(
    rally: &FormationRally,
    attention: &AttentionBroker,
    momentum: &mut MomentumState,
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

    // 2. Reduce momentum — the formation lost coherence
    momentum.record_failure();

    // 3. Consume rally token. `consume()` fails only if something closed
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
    use std::time::Duration;

    fn make_report(agent: AgentId, alignment: f32) -> TickReport {
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

        let result = attempt_self_rally(&rally, &attention, &mut momentum, a);
        assert!(matches!(
            result,
            RallyResult::StabilizedWithCost {
                tokens_remaining: 2
            }
        ));
        assert_eq!(rally.tokens.remaining(), 2);
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
        let mut momentum = MomentumState::default();

        let result = attempt_self_rally(&rally, &attention, &mut momentum, a);
        assert!(matches!(result, RallyResult::EscalateToOrchestrator { .. }));
    }
}
