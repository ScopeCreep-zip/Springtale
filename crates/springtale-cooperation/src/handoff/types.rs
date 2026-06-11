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

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::cadence::AgentId;
use crate::capability::CapabilityDecl;
use crate::types::WorkspaceKey;

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
        deposit_location: WorkspaceKey,
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
        next_capability_required: CapabilityDecl,
    },

    /// Sequential dependency. Splinter Cell boost.
    /// Agent A enables B, then B must enable A.
    SequentialDependency {
        enabler: AgentId,
        enabled: AgentId,
        return_obligation: crate::cadence::ActionDescriptor,
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
/// From COOPERATION.pdf §20.2. `Serialize + Deserialize` per plan §10.5 —
/// this is the wire contract when payloads cross a `FlexibleChainPool`
/// stealer, an environment-mediated deposit, or (eventually) a Veilid
/// edge. Expiry uses `DateTime<Utc>` rather than `Instant` so the
/// deadline survives serialization.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct HandoffPayload {
    pub data: serde_json::Value,
    pub schema: String,
    pub produced_by: crate::cadence::ActionDescriptor,
    pub consumable_by: Vec<CapabilityDecl>,
    pub expires: Option<DateTime<Utc>>,
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
            produced_by: crate::cadence::ActionDescriptor {
                kind: "slack_reader".to_owned(),
                target: None,
                payload_hash: 0,
            },
            consumable_by: vec!["github_writer".into()],
            expires: None,
        };

        let direct = HandoffType::Direct {
            sender: a,
            receiver: b,
            payload,
        };
        assert!(
            matches!(direct, HandoffType::Direct { sender, receiver, .. } if sender == a && receiver == b)
        );

        let info = HandoffType::InformationTransfer {
            source: a,
            recipients: vec![b],
            intelligence: "PR #42 needs review".into(),
            perishable: true,
        };
        assert!(
            matches!(info, HandoffType::InformationTransfer { perishable, ref recipients, .. } if perishable && recipients.len() == 1)
        );

        let dep = HandoffType::SequentialDependency {
            enabler: a,
            enabled: b,
            return_obligation: crate::cadence::ActionDescriptor {
                kind: "notify_completion".to_owned(),
                target: None,
                payload_hash: 0,
            },
        };
        assert!(
            matches!(dep, HandoffType::SequentialDependency { enabler, enabled, .. } if enabler == a && enabled == b)
        );
    }
}
