//! Step 7b — emit one structured tracing event per detected interference.
//!
//! Interference detection itself runs in step 2 (`build_reports`). This step
//! exists separately so observability can be tuned without touching the
//! detector. Per `docs/intended-arch/COOPERATION_SECURITY_REVIEW.md`, every
//! detected conflict must be observable so operators can audit
//! cooperative-failure incidents.

use crate::cooperation::formation::Formation;
use springtale_cooperation::tick_processor::FormationTickResult;

pub fn run(formation: &Formation, result: &FormationTickResult) {
    for event in &result.interferences {
        tracing::warn!(
            formation = %formation.id.0,
            agent_a = %event.agent_a.0,
            agent_b = %event.agent_b.0,
            severity = event.severity,
            "interference detected between agents"
        );
    }
}
