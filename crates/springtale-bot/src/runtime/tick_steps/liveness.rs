//! Step 4c — per-member liveness probe (Kubernetes pattern).
//!
//! Aligned report → `Alive`; missed three or more ticks → `Suspect { missed_ticks }`.
//! `Down` is set later by the supervisor (`SupervisionAction::MarkDown`,
//! handled in `supervision::run`).

use crate::cooperation::formation::Formation;
use springtale_cooperation::cadence::Tick;
use springtale_cooperation::supervision::Liveness;
use springtale_cooperation::tick_processor::FormationTickResult;

pub fn run(formation: &mut Formation, tick: &Tick, result: &FormationTickResult) {
    for report in &result.reports {
        if let Some(member) = formation.member_mut(&report.agent_id) {
            member.last_report_tick = tick.sequence;
            if report.intent_alignment > 0.5 {
                member.liveness = Liveness::Alive;
            }
        }
    }
    for member in &mut formation.members {
        if member.last_report_tick + 3 < tick.sequence && member.is_operational() {
            member.liveness = Liveness::Suspect {
                missed_ticks: (tick.sequence - member.last_report_tick) as u32,
            };
        }
    }
}
