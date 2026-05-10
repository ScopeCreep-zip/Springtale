//! Apply a `RecoveryAction` to an agent's `AgentHealth` — the §18.2
//! escalating-fragility FSM.
//!
//! The spec (COOPERATION.md §18.2, L4D-inspired table):
//!
//! | recovery_count | state        | next quick-fix outcome     |
//! |:--------------:|-------------|---------------------------|
//! | 0              | Operational | Degraded{1}                |
//! | 1              | Degraded{1} | Degraded{2}                |
//! | 2              | Degraded{2} | Dead{recoverable:true}    |
//! | n (proper)     | any         | Operational (counter = 0) |
//!
//! Quick-fix recovery (peer revive, byproduct, formation pulse) increments
//! the counter; proper recovery (environmental, redeployment) resets it.
//! Proactive protection doesn't touch health directly — it's a posture
//! change, not a heal — so `apply` leaves the target unchanged.
//!
//! `MAX_QUICK_FIX_COUNT = 2` matches the spec table: the third quick-fix
//! attempt kills the agent rather than restoring it.

use crate::recovery::RecoveryAction;
use crate::types::AgentHealth;

/// Classification of a recovery action for escalating-fragility accounting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryKind {
    /// Quick-fix: band-aid healing that increments the recovery counter.
    /// Peer revive, byproduct, formation pulse.
    QuickFix,
    /// Proper recovery: restores the agent and resets the counter.
    /// Environmental convergence, redeployment.
    Proper,
    /// Pure protection: doesn't heal, just buffs / changes posture. Leaves
    /// `AgentHealth` unchanged.
    Protective,
}

/// Maximum number of quick-fixes an agent can absorb before the next
/// quick-fix kills them instead. Per §18.2: the third attempt transitions
/// to `Dead{recoverable:true}`.
pub const MAX_QUICK_FIX_COUNT: u32 = 2;

impl RecoveryAction {
    /// Classify this action for the escalating-fragility counter.
    pub fn kind(&self) -> RecoveryKind {
        match self {
            RecoveryAction::PeerRevive { .. }
            | RecoveryAction::ByproductRecovery { .. }
            | RecoveryAction::FormationPulse { .. } => RecoveryKind::QuickFix,
            RecoveryAction::EnvironmentalRecovery { .. }
            | RecoveryAction::Redeployment { .. } => RecoveryKind::Proper,
            RecoveryAction::ProactiveProtection { .. } => RecoveryKind::Protective,
        }
    }

    /// Apply this recovery action to a target's current health and return
    /// the new state. Pure function — the caller writes back.
    ///
    /// See module docstring for the state table. Summary:
    /// - QuickFix: Operational → Degraded{1}; Degraded{n} → Degraded{n+1}
    ///   (n+1 > MAX → Dead); Incapacitated → Degraded{1}; Dead unchanged.
    /// - Proper: Degraded/Incapacitated → Operational; Redeployment on
    ///   Dead{recoverable:true} → Operational; unrecoverable Dead stays dead.
    /// - Protective: unchanged.
    pub fn apply(&self, current: AgentHealth) -> AgentHealth {
        match self.kind() {
            RecoveryKind::QuickFix => apply_quick_fix(current),
            RecoveryKind::Proper => apply_proper(self, current),
            RecoveryKind::Protective => current,
        }
    }
}

fn apply_quick_fix(current: AgentHealth) -> AgentHealth {
    match current {
        AgentHealth::Operational => AgentHealth::Degraded { recovery_count: 1 },
        AgentHealth::Degraded { recovery_count } => {
            let next = recovery_count + 1;
            if next > MAX_QUICK_FIX_COUNT {
                AgentHealth::Dead { recoverable: true }
            } else {
                AgentHealth::Degraded {
                    recovery_count: next,
                }
            }
        }
        AgentHealth::Incapacitated => AgentHealth::Degraded { recovery_count: 1 },
        AgentHealth::Dead { recoverable } => AgentHealth::Dead { recoverable },
    }
}

fn apply_proper(action: &RecoveryAction, current: AgentHealth) -> AgentHealth {
    match (action, current) {
        // Redeployment replaces the agent wholesale: full operational
        // state even from Dead{recoverable:true}. Unrecoverable Dead
        // stays dead — redeployment still can't revive what's lost.
        (
            RecoveryAction::Redeployment { .. },
            AgentHealth::Dead { recoverable: false },
        ) => AgentHealth::Dead { recoverable: false },
        (RecoveryAction::Redeployment { .. }, _) => AgentHealth::Operational,

        // Environmental recovery restores Degraded/Incapacitated but
        // can't reach a Dead agent (they need Redeployment).
        (
            RecoveryAction::EnvironmentalRecovery { .. },
            AgentHealth::Dead { recoverable },
        ) => AgentHealth::Dead { recoverable },
        (RecoveryAction::EnvironmentalRecovery { .. }, _) => AgentHealth::Operational,

        // Kind() narrowed to Proper — the remaining arms are unreachable
        // but we keep the target unchanged rather than panicking.
        (_, state) => state,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::cadence::{ActionDescriptor, AgentId};
    use crate::recovery::{ProtectionType, RecoveryAction, RecoveryCost};
    use crate::types::{AgentHealth, FuelAmount, ResourceId};
    use std::time::Duration;

    fn peer_revive() -> RecoveryAction {
        RecoveryAction::PeerRevive {
            healer: AgentId::new(),
            target: AgentId::new(),
            duration: Duration::from_secs(5),
            healer_vulnerability: 0.5,
        }
    }

    fn byproduct() -> RecoveryAction {
        RecoveryAction::ByproductRecovery {
            source: AgentId::new(),
            beneficiaries: vec![],
            recovery_amount: 0.3,
            primary_action: ActionDescriptor {
                kind: "work".into(),
                target: None,
                payload_hash: 0,
            },
        }
    }

    fn environmental() -> RecoveryAction {
        RecoveryAction::EnvironmentalRecovery {
            source_resource: ResourceId::from("well-1"),
            beneficiary: AgentId::new(),
            depletes_resource: false,
        }
    }

    fn redeploy() -> RecoveryAction {
        RecoveryAction::Redeployment {
            dead_agent: AgentId::new(),
            replacement_capabilities: vec![],
            cost: RecoveryCost::Free,
            degraded: false,
        }
    }

    fn protective() -> RecoveryAction {
        RecoveryAction::ProactiveProtection {
            protector: AgentId::new(),
            beneficiaries: vec![],
            protection_type: ProtectionType::DamageShield {
                duration: Duration::from_secs(5),
            },
        }
    }

    #[test]
    fn quickfix_from_operational_goes_to_degraded_1() {
        let next = peer_revive().apply(AgentHealth::Operational);
        assert!(matches!(
            next,
            AgentHealth::Degraded { recovery_count: 1 }
        ));
    }

    #[test]
    fn quickfix_increments_counter() {
        let next = byproduct().apply(AgentHealth::Degraded { recovery_count: 1 });
        assert!(matches!(
            next,
            AgentHealth::Degraded { recovery_count: 2 }
        ));
    }

    #[test]
    fn third_quickfix_kills_the_agent() {
        // Agent at Degraded{2} (already quick-fixed twice): one more
        // pushes them past MAX_QUICK_FIX_COUNT and they die.
        let next = peer_revive().apply(AgentHealth::Degraded { recovery_count: 2 });
        assert!(matches!(next, AgentHealth::Dead { recoverable: true }));
    }

    #[test]
    fn quickfix_lifts_incapacitated_to_degraded_1() {
        let next = peer_revive().apply(AgentHealth::Incapacitated);
        assert!(matches!(
            next,
            AgentHealth::Degraded { recovery_count: 1 }
        ));
    }

    #[test]
    fn proper_environmental_resets_counter() {
        let next = environmental().apply(AgentHealth::Degraded { recovery_count: 2 });
        assert!(matches!(next, AgentHealth::Operational));
    }

    #[test]
    fn proper_environmental_does_not_revive_dead() {
        let next = environmental().apply(AgentHealth::Dead { recoverable: true });
        assert!(matches!(next, AgentHealth::Dead { recoverable: true }));
    }

    #[test]
    fn redeployment_restores_recoverable_dead() {
        let next = redeploy().apply(AgentHealth::Dead { recoverable: true });
        assert!(matches!(next, AgentHealth::Operational));
    }

    #[test]
    fn redeployment_cannot_restore_unrecoverable_dead() {
        let next = redeploy().apply(AgentHealth::Dead { recoverable: false });
        assert!(matches!(next, AgentHealth::Dead { recoverable: false }));
    }

    #[test]
    fn protective_leaves_health_unchanged() {
        let next = protective().apply(AgentHealth::Degraded { recovery_count: 1 });
        assert!(matches!(
            next,
            AgentHealth::Degraded { recovery_count: 1 }
        ));
    }

    #[test]
    fn kind_classifies_actions() {
        assert_eq!(peer_revive().kind(), RecoveryKind::QuickFix);
        assert_eq!(byproduct().kind(), RecoveryKind::QuickFix);
        assert_eq!(
            RecoveryAction::FormationPulse {
                source: AgentId::new(),
                recovery_amount: 0.5,
                cost: RecoveryCost::Fuel(FuelAmount(1)),
            }
            .kind(),
            RecoveryKind::QuickFix
        );
        assert_eq!(environmental().kind(), RecoveryKind::Proper);
        assert_eq!(redeploy().kind(), RecoveryKind::Proper);
        assert_eq!(protective().kind(), RecoveryKind::Protective);
    }

    #[test]
    fn quickfix_on_dead_agent_stays_dead() {
        // You can't quick-fix a dead agent — the next state is whatever
        // they already were.
        let next = peer_revive().apply(AgentHealth::Dead { recoverable: false });
        assert!(matches!(next, AgentHealth::Dead { recoverable: false }));
    }
}
