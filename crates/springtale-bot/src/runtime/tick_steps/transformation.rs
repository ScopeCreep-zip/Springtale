//! Step 10 — role transformation for failing/dead members (`COOPERATION.md §14`).
//!
//! Evaluates ALL members (not just non-operational); rule 3 (5+ failures)
//! applies to operational agents that keep failing. The chosen role is
//! materialized via `apply_transformation_via_registry` so community roles
//! contributed by connector manifests can win over the built-in factories
//! when same-named (§14.4 / Phase 21).

use crate::cooperation::formation::Formation;
use springtale_cooperation::role::{RoleRegistry, apply_transformation_via_registry};
use springtale_cooperation::transformation::trigger;

pub fn run(formation: &mut Formation, role_registry: &RoleRegistry) {
    for member in &mut formation.members {
        let caps = springtale_cooperation::capability::DynamicCapabilitySet {
            base_capabilities: member.capabilities.clone(),
            context_capabilities: vec![],
            momentum_unlocked: vec![],
            transformed_capabilities: vec![],
        };
        if let Some(transformation) = trigger::evaluate_transformation(
            &member.health,
            &caps,
            member.consecutive_failures,
        ) {
            member.role = apply_transformation_via_registry(
                role_registry,
                &member.capabilities,
                &transformation,
            );
            tracing::info!(
                formation = %formation.id.0,
                agent = %member.agent_id.0,
                role = member.role.name(),
                "agent role transformed"
            );
        }
    }
}
