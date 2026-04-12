//! Handoff & transition — work product transfer between agents.
//!
//! Per COOPERATION.pdf §20:
//! "Work products must pass between agents. The handoff point is where
//! most cooperative failures occur."
//!
//! Five handoff patterns from different games:
//! - Direct (Overcooked: pass ingredient across counter)
//! - Environment-mediated (Divinity surfaces, MH monster state)
//! - Flexible chain (DRG minerals: any capable agent can do next step)
//! - Sequential dependency (Splinter Cell boost: A enables B, B must enable A)
//! - Information transfer (Siege callouts: no physical payload, just knowledge)

use std::time::Instant;

use super::cadence::AgentId;

/// How work products transfer between agents.
///
/// From COOPERATION.pdf §20.2:
pub enum HandoffType {
    /// Direct transfer. Overcooked: pass ingredient across counter.
    /// Synchronous — both agents must be ready.
    Direct {
        sender: AgentId,
        receiver: AgentId,
        payload: HandoffPayload,
    },

    /// Environment-mediated. Divinity surfaces, MH monster state.
    /// Asynchronous — sender deposits, receiver collects when ready.
    EnvironmentMediated {
        depositor: AgentId,
        deposit_location: String,
        payload: HandoffPayload,
        transform_required: Option<String>,
    },

    /// Flexible chain. DRG minerals.
    /// Any capable agent can perform the next step.
    FlexibleChain {
        originator: AgentId,
        current_step: usize,
        total_steps: usize,
        payload: HandoffPayload,
        next_capability_required: String,
    },

    /// Sequential dependency. Splinter Cell boost.
    /// Agent A enables B, then B must enable A.
    SequentialDependency {
        enabler: AgentId,
        enabled: AgentId,
        return_obligation: String,
    },

    /// Information handoff. Siege callouts.
    /// No physical payload — knowledge transfer.
    InformationTransfer {
        source: AgentId,
        recipients: Vec<AgentId>,
        intelligence: String,
        perishable: bool, // does this info expire quickly?
    },
}

/// The data being transferred in a handoff.
///
/// From COOPERATION.pdf §20.2:
pub struct HandoffPayload {
    pub data: serde_json::Value,
    pub schema: String,
    pub produced_by: String,
    pub consumable_by: Vec<String>,
    pub expires: Option<Instant>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_handoff_types() {
        let a = AgentId::new();
        let b = AgentId::new();
        let payload = HandoffPayload {
            data: serde_json::json!({"result": "processed"}),
            schema: "slack_message".into(),
            produced_by: "slack_reader".into(),
            consumable_by: vec!["github_writer".into()],
            expires: None,
        };

        let _direct = HandoffType::Direct {
            sender: a,
            receiver: b,
            payload,
        };

        let _info = HandoffType::InformationTransfer {
            source: a,
            recipients: vec![b],
            intelligence: "PR #42 needs review".into(),
            perishable: true,
        };

        let _dep = HandoffType::SequentialDependency {
            enabler: a,
            enabled: b,
            return_obligation: "notify_completion".into(),
        };
    }
}
