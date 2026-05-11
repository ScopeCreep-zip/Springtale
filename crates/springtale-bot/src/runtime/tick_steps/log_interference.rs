//! Step 7b — emit one structured tracing event per detected interference,
//! and (Phase H5) one `CooperationEvent::InterferenceDetected` envelope to
//! the cooperation events bus.
//!
//! Interference detection itself runs in step 2 (`build_reports`). This step
//! exists separately so observability can be tuned without touching the
//! detector. Per `docs/intended-arch/COOPERATION_SECURITY_REVIEW.md`, every
//! detected conflict must be observable so operators can audit
//! cooperative-failure incidents.

use tokio::sync::broadcast;

use crate::cooperation::formation::Formation;
use springtale_cooperation::events::{
    self, CooperationEvent, CooperationEventEnvelope, InterferenceKind,
};
use springtale_cooperation::interference::InterferenceType;
use springtale_cooperation::tick_processor::FormationTickResult;

pub fn run(
    formation: &Formation,
    result: &FormationTickResult,
    cooperation_tx: Option<&broadcast::Sender<CooperationEventEnvelope>>,
) {
    for event in &result.interferences {
        tracing::warn!(
            formation = %formation.id.0,
            agent_a = %event.agent_a.0,
            agent_b = %event.agent_b.0,
            severity = event.severity,
            "interference detected between agents"
        );
        let kind = match event.interference_type {
            InterferenceType::ResourceConflict => InterferenceKind::ResourceConflict,
            InterferenceType::ActionNegation => InterferenceKind::ActionNegation,
            InterferenceType::CollateralDamage => InterferenceKind::CollateralDamage,
            InterferenceType::Redundancy => InterferenceKind::DuplicateAction,
        };
        events::emit(
            cooperation_tx,
            CooperationEvent::InterferenceDetected {
                formation_id: formation.id,
                interference_kind: kind,
                agents: vec![event.agent_a, event.agent_b],
            },
        );
    }
}
