//! Sacrifice & covering — deliberate self-cost for formation benefit.
//!
//! Per COOPERATION.pdf §24:
//! "Distinct from recovery (§18), which helps agents already in trouble.
//! Sacrifice is an agent deliberately accepting cost BEFORE failure occurs
//! to benefit the formation."
//!
//! Sacrifice must be VOLUNTARY (agent decides based on local awareness),
//! not COMMANDED (orchestrator ordering sacrifice is micromanagement).
//! Cooperative sacrifice is mutual aid.
//!
//! Game patterns:
//! - Total War: Screening force (cheap unit holds position to buy time)
//! - Siege: Entry fragging (first through door accepts high death rate)
//! - MH: Aggro drawing (take hits to protect healing teammate)
//! - Army of Two: Feigned death (crawl defenseless to enable flanking)
//! - Helldivers: Self-bombing (orbital strike own position, trust reinforce)
//! - DRG: Gunner shield (spend own resource to protect formation)
//! - L4D: Body blocking (position between threat and wounded teammate)
//! - Patapon: Defend rhythm (zero offense for damage prevention)
//! - Overcooked: Station covering (leave your station to help overwhelmed partner)
//! - Divinity: Healing over damage (AP spent healing = AP not attacking)

use super::cadence::AgentId;

/// Types of sacrifice an agent can make.
///
/// From COOPERATION.pdf §24.2:
pub enum SacrificeType {
    /// Accept damage/cost to protect another agent.
    /// MH aggro drawing, L4D body blocking, DRG shield.
    Covering {
        sacrificer: AgentId,
        beneficiary: AgentId,
        cost_to_sacrificer: SacrificeCost,
        benefit_to_beneficiary: String,
    },

    /// Accept task degradation to assist another agent's task.
    /// Overcooked station covering, Siege entry fragging.
    TaskDiversion {
        sacrificer: AgentId,
        abandoned_task: String,
        assumed_task: String,
        formation_net_benefit: f32, // must be positive to justify
    },

    /// Accept individual destruction for formation benefit.
    /// Total War screening force, Helldivers self-bombing.
    Expendable {
        sacrificer: AgentId,
        expected_recovery: Option<String>, // Helldivers: reinforce
        formation_benefit: String,
    },

    /// Spend own resources on formation infrastructure.
    /// DRG Gunner shield, Patapon defend rhythm.
    ResourceInvestment {
        investor: AgentId,
        resource_spent: String,
        infrastructure_created: String,
        beneficiaries: Vec<AgentId>,
    },
}

/// What the sacrifice costs the sacrificing agent.
pub struct SacrificeCost {
    pub fuel_cost: u64,
    pub capability_reduction: Vec<String>, // capabilities temporarily lost
    pub vulnerability_increase: f32,
    pub duration: std::time::Duration,
}

// Sacrifice Decision Framework (§24.3):
// An agent evaluates using attention economy (§9) and awareness (§8):
// 1. Net positive check: Does the formation's total output improve?
// 2. Recovery path check: Is there a way back? (Helldivers reinforce exists)
// 3. Capability preservation: Does the sacrifice eliminate a unique capability?
// 4. Momentum impact: Does the sacrifice risk breaking momentum?

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_sacrifice_types() {
        let a = AgentId::new();
        let b = AgentId::new();

        let _covering = SacrificeType::Covering {
            sacrificer: a,
            beneficiary: b,
            cost_to_sacrificer: SacrificeCost {
                fuel_cost: 100,
                capability_reduction: vec!["primary_action".into()],
                vulnerability_increase: 0.5,
                duration: Duration::from_secs(10),
            },
            benefit_to_beneficiary: "safe recovery window".into(),
        };

        let _diversion = SacrificeType::TaskDiversion {
            sacrificer: a,
            abandoned_task: "process_queue".into(),
            assumed_task: "cover_failing_agent".into(),
            formation_net_benefit: 0.3,
        };

        let _expendable = SacrificeType::Expendable {
            sacrificer: a,
            expected_recovery: Some("redeployment".into()),
            formation_benefit: "cleared bottleneck".into(),
        };

        let _investment = SacrificeType::ResourceInvestment {
            investor: a,
            resource_spent: "shield_charge".into(),
            infrastructure_created: "safe_zone".into(),
            beneficiaries: vec![b],
        };
    }
}
