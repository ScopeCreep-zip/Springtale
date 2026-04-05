//! Constraints — boundaries that formations cannot exceed.
//!
//! Per COOPERATION.pdf §3.3:
//! Game source: Total War guard/free mode, Siege round timer,
//! sentinel controls.
//!
//! The full FormationConstraints from the spec includes fields
//! beyond what cooperation::formation::FormationConstraints has.
//! This module defines the orchestrator-level constraint types
//! that wrap the sentinel's existing controls.

use std::time::Duration;

use super::fuel::FuelBudget;

/// How destructive actions should be handled.
///
/// Per ARCHITECTURE.md autonomy levels: destructive = always L1 (human approval).
pub enum ApprovalPolicy {
    /// Always require human approval (L1).
    AlwaysApprove,
    /// Allow if momentum is at Fever tier.
    AllowAtFever,
    /// Block entirely — formation cannot perform destructive actions.
    Block,
}

/// Maximum autonomy level for a formation.
///
/// Maps to ARCHITECTURE.md §2 autonomy levels.
pub enum AutonomyLevel {
    /// L1: Human approves every action.
    HumanApproval,
    /// L2: Human approves plans, agent executes steps.
    PlanApproval,
    /// L3: Agent acts, human monitors.
    Monitored,
    /// L4: Full autonomy within constraints.
    FullAutonomy,
}

/// Full orchestrator-level constraints for a formation.
///
/// From COOPERATION.pdf §3.3:
/// ```text
/// pub struct FormationConstraints {
///     pub fuel_budget: FuelBudget,
///     pub timeout: Duration,
///     pub max_concurrent_actions: usize,
///     pub destructive_action_policy: ApprovalPolicy,
///     pub guard_mode: bool,
///     pub autonomy_ceiling: AutonomyLevel,
/// }
/// ```
pub struct OrchestratorConstraints {
    pub fuel_budget: FuelBudget,
    pub timeout: Duration,
    pub max_concurrent_actions: usize,
    pub destructive_action_policy: ApprovalPolicy,
    pub guard_mode: bool,
    pub autonomy_ceiling: AutonomyLevel,
}

impl Default for OrchestratorConstraints {
    fn default() -> Self {
        Self {
            fuel_budget: FuelBudget::new(10_000_000),
            timeout: Duration::from_secs(300),
            max_concurrent_actions: 8,
            destructive_action_policy: ApprovalPolicy::AlwaysApprove,
            guard_mode: false,
            autonomy_ceiling: AutonomyLevel::PlanApproval,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_default_constraints() {
        let c = OrchestratorConstraints::default();
        assert!(!c.guard_mode);
        assert!(matches!(c.destructive_action_policy, ApprovalPolicy::AlwaysApprove));
        assert!(matches!(c.autonomy_ceiling, AutonomyLevel::PlanApproval));
    }
}
