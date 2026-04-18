//! Transformation triggers — evaluate when an agent should change roles.
//!
//! Per COOPERATION.pdf §14:
//! - Siege: dead → cameras (ToInformationAgent)
//! - Army of Two: low-aggro → overwatch (ToSupportAgent)
//! - It Takes Two: chapter abilities (ReassignCapabilities)

use crate::capability::DynamicCapabilitySet;
use crate::types::AgentHealth;

use super::RoleTransformation;

/// Evaluate whether an agent should transform roles.
///
/// Called per tick for members whose tick report indicates failure
/// or health change. Returns None if no transformation needed.
///
/// Rules (evaluated in priority order):
/// 1. Dead/Incapacitated → ToInformationAgent (Siege dead→intel)
/// 2. All primary capabilities exhausted → ToSupportAgent
/// 3. Repeated failure (threshold consecutive failures) → ToSupportAgent
pub fn evaluate_transformation(
    health: &AgentHealth,
    capabilities: &DynamicCapabilitySet,
    consecutive_failures: usize,
) -> Option<RoleTransformation> {
    // Rule 1: Dead or incapacitated → information agent
    // Per Siege: dead players switch to cameras and provide callouts
    if matches!(
        health,
        AgentHealth::Dead { .. } | AgentHealth::Incapacitated
    ) {
        return Some(RoleTransformation::ToInformationAgent);
    }

    // Rule 2: No base capabilities remaining → support agent
    // Per Army of Two: low-aggro player shifts to overwatch
    if capabilities.base_capabilities.is_empty() {
        return Some(RoleTransformation::ToSupportAgent);
    }

    // Rule 3: Repeated failure → support agent
    // Threshold: 5 consecutive failures indicates this agent can't execute
    // its current role effectively
    if consecutive_failures >= 5 {
        return Some(RoleTransformation::ToSupportAgent);
    }

    None
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn make_caps(base: Vec<&str>) -> DynamicCapabilitySet {
        use crate::capability::CapabilityDecl;
        DynamicCapabilitySet {
            base_capabilities: base.into_iter().map(CapabilityDecl::new).collect(),
            context_capabilities: vec![],
            momentum_unlocked: vec![],
            transformed_capabilities: vec![],
        }
    }

    #[test]
    fn test_dead_becomes_information() {
        let result = evaluate_transformation(
            &AgentHealth::Dead { recoverable: true },
            &make_caps(vec!["slack_send"]),
            0,
        );
        assert!(matches!(result, Some(RoleTransformation::ToInformationAgent)));
    }

    #[test]
    fn test_incapacitated_becomes_information() {
        let result = evaluate_transformation(
            &AgentHealth::Incapacitated,
            &make_caps(vec!["slack_send"]),
            0,
        );
        assert!(matches!(result, Some(RoleTransformation::ToInformationAgent)));
    }

    #[test]
    fn test_no_capabilities_becomes_support() {
        let result = evaluate_transformation(
            &AgentHealth::Operational,
            &make_caps(vec![]),
            0,
        );
        assert!(matches!(result, Some(RoleTransformation::ToSupportAgent)));
    }

    #[test]
    fn test_repeated_failure_becomes_support() {
        let result = evaluate_transformation(
            &AgentHealth::Operational,
            &make_caps(vec!["slack_send"]),
            5,
        );
        assert!(matches!(result, Some(RoleTransformation::ToSupportAgent)));
    }

    #[test]
    fn test_healthy_agent_no_transformation() {
        let result = evaluate_transformation(
            &AgentHealth::Operational,
            &make_caps(vec!["slack_send"]),
            2,
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_degraded_agent_no_transformation() {
        let result = evaluate_transformation(
            &AgentHealth::Degraded { recovery_count: 1 },
            &make_caps(vec!["slack_send"]),
            3,
        );
        assert!(result.is_none());
    }
}
