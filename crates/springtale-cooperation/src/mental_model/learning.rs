//! Mental model learning — update the shared model from tick observations.
//!
//! Per COOPERATION.pdf §21:
//! - L4D: Cultural preload (everyone knows zombie movies)
//! - Siege: Accumulated map knowledge (callout vocabulary)
//! - MH: Pattern recognition (monster attack windows)
//! - DRG: Class capability awareness ("I know what you can do")

use std::time::Instant;

use crate::cadence::TickReport;
use crate::interference::InterferenceEvent;

use super::{CooperationPattern, SharedMentalModel};

/// Update the shared mental model after a tick.
///
/// Called per tick after interference detection. Learns from:
/// 1. Capability awareness — what each agent actually did this tick
/// 2. Cooperation patterns — successful multi-agent chains
/// 3. Convention strength — reinforce successful patterns, weaken failed ones
pub fn update_model(
    model: &mut SharedMentalModel,
    reports: &[TickReport],
    interferences: &[InterferenceEvent],
    all_succeeded: bool,
) {
    // 1. Update capability awareness from tick reports
    //    "I know what you can do" (DRG pattern)
    for report in reports {
        if let Some(action) = &report.action_taken {
            let caps = model
                .capability_awareness
                .entry(report.agent_id)
                .or_default();
            if !caps.iter().any(|c| c.name == action.kind) {
                caps.push(crate::capability::CapabilityDecl::new(action.kind.clone()));
            }
        }
    }

    // 2. Record cooperation pattern if 2+ agents successfully acted
    //    on the same tick without interference
    if all_succeeded && reports.len() >= 2 {
        let acting_agents: Vec<_> = reports
            .iter()
            .filter(|r| r.action_taken.is_some())
            .collect();

        if acting_agents.len() >= 2 {
            let trigger = acting_agents
                .iter()
                .filter_map(|r| r.action_taken.as_ref().map(|a| a.kind.as_str()))
                .collect::<Vec<_>>()
                .join("+");

            let participants: Vec<_> = acting_agents.iter().map(|r| r.agent_id).collect();

            // Find existing pattern or create new one
            let trigger_id: crate::types::PatternId = trigger.into();
            let existing = model
                .cooperation_patterns
                .iter_mut()
                .find(|p| p.trigger == trigger_id);

            if let Some(pattern) = existing {
                pattern.success_count += 1;
                pattern.last_used = Instant::now();
            } else {
                model.cooperation_patterns.push(CooperationPattern {
                    trigger: trigger_id,
                    participants,
                    success_count: 1,
                    failure_count: 0,
                    last_used: Instant::now(),
                });
            }
        }
    }

    // 3. Record interference as failed pattern
    for event in interferences {
        let trigger = format!(
            "interference:{}",
            match &event.interference_type {
                crate::interference::InterferenceType::ResourceConflict => "resource_conflict",
                crate::interference::InterferenceType::ActionNegation => "action_negation",
                crate::interference::InterferenceType::CollateralDamage => "collateral_damage",
                crate::interference::InterferenceType::Redundancy => "redundancy",
            }
        );

        let trigger_id: crate::types::PatternId = trigger.into();
        let existing = model
            .cooperation_patterns
            .iter_mut()
            .find(|p| p.trigger == trigger_id);

        if let Some(pattern) = existing {
            pattern.failure_count += 1;
            pattern.last_used = Instant::now();
        } else {
            model.cooperation_patterns.push(CooperationPattern {
                trigger: trigger_id,
                participants: vec![event.agent_a, event.agent_b],
                success_count: 0,
                failure_count: 1,
                last_used: Instant::now(),
            });
        }
    }

    // 4. Strengthen conventions from successful ticks, weaken from failed
    for convention in model.conventions.iter_mut() {
        if all_succeeded {
            convention.strength = (convention.strength + 0.05).min(1.0);
        } else {
            convention.strength = (convention.strength - 0.02).max(0.0);
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::cadence::AgentId;
    use crate::mental_model::Convention;
    use std::time::Duration;

    fn make_report(agent: AgentId, action: Option<&str>) -> TickReport {
        TickReport {
            agent_id: agent,
            tick_sequence: crate::tick::TickId(1),
            action_taken: action.map(|s| crate::cadence::ActionDescriptor {
                kind: s.to_owned(),
                target: None,
                payload_hash: 0,
            }),
            latency: Duration::from_millis(5),
            intent_alignment: 0.9,
            interference_with: vec![],
        }
    }

    #[test]
    fn test_capability_awareness_updated() {
        let mut model = SharedMentalModel::default();
        let a = AgentId::new();

        let reports = vec![make_report(a, Some("send_message"))];
        update_model(&mut model, &reports, &[], true);

        let caps = model.capability_awareness.get(&a).unwrap();
        assert_eq!(caps.len(), 1);
        assert_eq!(caps[0].name, "send_message");
    }

    #[test]
    fn test_capability_not_duplicated() {
        let mut model = SharedMentalModel::default();
        let a = AgentId::new();

        let reports = vec![make_report(a, Some("send_message"))];
        update_model(&mut model, &reports, &[], true);
        update_model(&mut model, &reports, &[], true);

        let caps = model.capability_awareness.get(&a).unwrap();
        assert_eq!(caps.len(), 1);
    }

    #[test]
    fn test_cooperation_pattern_recorded() {
        let mut model = SharedMentalModel::default();
        let a = AgentId::new();
        let b = AgentId::new();

        let reports = vec![
            make_report(a, Some("read_issues")),
            make_report(b, Some("send_notification")),
        ];
        update_model(&mut model, &reports, &[], true);

        assert_eq!(model.cooperation_patterns.len(), 1);
        assert_eq!(model.cooperation_patterns[0].success_count, 1);
        assert_eq!(
            model.cooperation_patterns[0].trigger,
            *"read_issues+send_notification"
        );
    }

    #[test]
    fn test_repeated_pattern_increments() {
        let mut model = SharedMentalModel::default();
        let a = AgentId::new();
        let b = AgentId::new();

        let reports = vec![
            make_report(a, Some("read_issues")),
            make_report(b, Some("send_notification")),
        ];
        update_model(&mut model, &reports, &[], true);
        update_model(&mut model, &reports, &[], true);

        assert_eq!(model.cooperation_patterns.len(), 1);
        assert_eq!(model.cooperation_patterns[0].success_count, 2);
    }

    #[test]
    fn test_interference_records_failure_pattern() {
        use crate::interference::{InterferenceEvent, InterferenceType};

        let mut model = SharedMentalModel::default();
        let a = AgentId::new();
        let b = AgentId::new();

        let interferences = vec![InterferenceEvent {
            tick_sequence: crate::tick::TickId(1),
            agent_a: a,
            agent_b: b,
            interference_type: InterferenceType::Redundancy,
            severity: 0.2,
        }];

        update_model(&mut model, &[], &interferences, false);

        assert_eq!(model.cooperation_patterns.len(), 1);
        assert_eq!(model.cooperation_patterns[0].failure_count, 1);
        assert_eq!(
            model.cooperation_patterns[0].trigger,
            *"interference:redundancy"
        );
    }

    #[test]
    fn test_convention_strength_changes() {
        let mut model = SharedMentalModel::default();
        model.conventions.push(Convention {
            description: "agent A handles slack".to_owned(),
            established_by: vec![AgentId::new()],
            strength: 0.5,
        });

        // Success strengthens
        update_model(&mut model, &[], &[], true);
        assert!((model.conventions[0].strength - 0.55).abs() < f32::EPSILON);

        // Failure weakens
        update_model(&mut model, &[], &[], false);
        assert!((model.conventions[0].strength - 0.53).abs() < f32::EPSILON);
    }
}
