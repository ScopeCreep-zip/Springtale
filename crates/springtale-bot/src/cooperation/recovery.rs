//! Recovery & mutual aid system — distress detection, recovery delivery, escalating fragility.
//!
//! Per COOPERATION.pdf §18:
//! "The other side of cooperation is agents actively seeking out and helping each other."
//!
//! Recovery has TIERS (L4D-inspired escalating fragility):
//! - Quick-fix (peer revive, byproduct) increments the recovery counter
//! - Proper recovery (environmental, convergence point) resets it
//! - Repeated quick-fixes without proper recovery → escalating fragility → death
//!
//! Recovery Decision Framework (§18.4) — each agent evaluates locally:
//! 1. Can I help? (capability check)
//! 2. Should I help? (cost vs value — Army of Two dilemma)
//! 3. Should someone else help? (proximity — MH nearest hunter revives)
//! 4. Should we prevent instead? (Patapon defend, DRG shield)
//! 5. Should we let them transform? (Siege dead→intel)

use std::time::Duration;

use super::cadence::AgentId;

/// How an agent signals it needs help.
///
/// From COOPERATION.pdf §18.2:
pub enum DistressSignal {
    /// Agent health below threshold. Total War: morale dropping.
    HealthLow { agent_id: AgentId, health_pct: f32 },
    /// Agent incapacitated. L4D: downed state. DRG: downed dwarf.
    Incapacitated {
        agent_id: AgentId,
        bleedout_remaining: Duration,
    },
    /// Agent dead/disconnected. Helldivers: needs reinforce.
    Dead {
        agent_id: AgentId,
        recoverable: bool,
    },
    /// Agent capability degraded. Siege: DBNO with limited actions.
    Degraded {
        agent_id: AgentId,
        remaining_capabilities: Vec<String>,
    },
}

/// How recovery is delivered.
///
/// From COOPERATION.pdf §18.2:
pub enum RecoveryAction {
    /// Peer revive — one agent directly restores another.
    /// Cost: the healer is vulnerable during the action.
    /// L4D medkit, DRG revive, Splinter Cell revive, Army of Two drag-heal.
    PeerRevive {
        healer: AgentId,
        target: AgentId,
        duration: Duration,
        healer_vulnerability: f32, // 0.0-1.0: how exposed the healer is
    },

    /// Byproduct recovery — agent heals neighbors by doing its normal work.
    /// MH Hunting Horn: attack combos apply healing melodies.
    /// MH Wide-Range: self-healing shares to team.
    /// Divinity Necromancy: dealing damage heals self.
    ByproductRecovery {
        source: AgentId,
        beneficiaries: Vec<AgentId>,
        recovery_amount: f32,
        primary_action: String, // the productive work that caused healing
    },

    /// Formation-wide pulse — all agents get a boost simultaneously.
    /// Siege Finka boost. Helldivers reinforce. Patapon defend rhythm.
    FormationPulse {
        source: AgentId,
        recovery_amount: f32,
        cost: RecoveryCost,
    },

    /// Environmental recovery — agent uses a shared resource.
    /// DRG Red Sugar. L4D safe room. Helldivers resupply convergence.
    EnvironmentalRecovery {
        source_resource: String,
        beneficiary: AgentId,
        depletes_resource: bool,
    },

    /// Redeployment — replace a dead agent entirely.
    /// Helldivers reinforce. L4D rescue closet.
    Redeployment {
        dead_agent: AgentId,
        replacement_capabilities: Vec<String>,
        cost: RecoveryCost,
        degraded: bool, // L4D rescue closet: Tier 1 weapons only
    },

    /// Proactive protection — change formation posture to prevent damage.
    /// Patapon defend rhythm. DRG Gunner shield. Rook armor plates.
    ProactiveProtection {
        protector: AgentId,
        beneficiaries: Vec<AgentId>,
        protection_type: ProtectionType,
    },
}

/// What recovery costs.
///
/// From COOPERATION.pdf §18.2:
pub enum RecoveryCost {
    /// Recovery costs fuel from the healer's budget.
    Fuel(u64),
    /// Recovery costs shared formation fuel.
    SharedFuel(u64),
    /// Recovery costs time (Overcooked error correction).
    Time(Duration),
    /// Recovery costs a scarce token (As Dusk Falls override, DRG Iron Will).
    Token {
        token_type: String,
        remaining_after: u32,
    },
    /// Recovery is free (It Takes Two checkpoint).
    Free,
}

/// Types of proactive protection.
///
/// From COOPERATION.pdf §18.2:
pub enum ProtectionType {
    /// Shield/barrier that blocks damage. DRG Gunner shield.
    DamageShield { duration: Duration },
    /// Posture change that reduces incoming damage. Patapon defend.
    PostureChange { damage_reduction: f32 },
    /// Preemptive buff that changes failure mode. Rook armor = DBNO not death.
    FailureModeChange { from: FailureMode, to: FailureMode },
}

/// How an agent fails.
///
/// From COOPERATION.pdf §18.2:
pub enum FailureMode {
    /// Agent goes from operational to dead. No recovery window.
    InstantDeath,
    /// Agent enters degraded state with recovery window. L4D incapacitation.
    Degraded { recovery_window: Duration },
    /// Agent transforms role instead of failing. Siege dead→intel.
    RoleTransformation,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_distress_signals() {
        let agent = AgentId::new();
        let _low = DistressSignal::HealthLow {
            agent_id: agent,
            health_pct: 0.2,
        };
        let _incap = DistressSignal::Incapacitated {
            agent_id: agent,
            bleedout_remaining: Duration::from_secs(30),
        };
        let _dead = DistressSignal::Dead {
            agent_id: agent,
            recoverable: true,
        };
        let _degraded = DistressSignal::Degraded {
            agent_id: agent,
            remaining_capabilities: vec!["monitoring".into()],
        };
    }

    #[test]
    fn test_recovery_actions() {
        let healer = AgentId::new();
        let target = AgentId::new();

        let _revive = RecoveryAction::PeerRevive {
            healer,
            target,
            duration: Duration::from_secs(5),
            healer_vulnerability: 0.8,
        };

        let _byproduct = RecoveryAction::ByproductRecovery {
            source: healer,
            beneficiaries: vec![target],
            recovery_amount: 0.3,
            primary_action: "process_queue".into(),
        };

        let _redeploy = RecoveryAction::Redeployment {
            dead_agent: target,
            replacement_capabilities: vec!["basic_monitoring".into()],
            cost: RecoveryCost::SharedFuel(500),
            degraded: true,
        };
    }

    #[test]
    fn test_failure_modes() {
        let _instant = FailureMode::InstantDeath;
        let _degraded = FailureMode::Degraded {
            recovery_window: Duration::from_secs(30),
        };
        let _transform = FailureMode::RoleTransformation;
    }
}
