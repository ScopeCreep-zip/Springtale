//! Step 4g + 4h — health threshold broadcasts and cohesion signals.
//!
//! 4g (`COOPERATION.md §19` `StateBroadcast` — L4D "I'm hurt pretty bad"):
//! when a member's health crosses Incapacitated or Degraded, broadcast on
//! the bus so peers can react without being explicitly told.
//!
//! 4h (`§19` Rock-and-Stone): on momentum tier change, emit one
//! `CohesionSignal` per member — morale event, no information content,
//! signals the formation's cadence has shifted.

use crate::cooperation::formation::Formation;
use springtale_cooperation::comms::{
    BroadcastTrigger, CohesionSignalMsg, StateBroadcastMsg, StateMessage,
};
use springtale_cooperation::tick_processor::FormationTickResult;
use springtale_cooperation::types::AgentHealth;

pub fn run(formation: &mut Formation, result: &FormationTickResult) {
    // 4g — health threshold broadcasts.
    for report in &result.reports {
        let Some(member) = formation.member(&report.agent_id) else {
            continue;
        };
        match member.health {
            AgentHealth::Incapacitated => {
                formation.bus.broadcast_state(StateBroadcastMsg {
                    source: member.agent_id,
                    trigger: BroadcastTrigger::AgentDown(member.agent_id),
                    message: StateMessage {
                        content: "incapacitated".to_owned(),
                        severity: 0.9,
                    },
                });
            }
            AgentHealth::Degraded { recovery_count } => {
                let fuel_pct = if formation.fuel.initial() > 0 {
                    formation.fuel.remaining() as f32 / formation.fuel.initial() as f32
                } else {
                    1.0
                };
                formation.bus.broadcast_state(StateBroadcastMsg {
                    source: member.agent_id,
                    trigger: BroadcastTrigger::HealthBelowThreshold(fuel_pct),
                    message: StateMessage {
                        content: format!("degraded (recovery_count = {recovery_count})"),
                        severity: 0.5,
                    },
                });
            }
            _ => {}
        }
    }

    // 4h — cohesion signal on tier change.
    let current_tier = formation.momentum.tier;
    if current_tier != formation.last_broadcast_tier {
        for member in &formation.members {
            formation.bus.signal_cohesion(CohesionSignalMsg {
                source: member.agent_id,
            });
        }
        formation.last_broadcast_tier = current_tier;
    }
}
