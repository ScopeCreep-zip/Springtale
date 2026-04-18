//! Handoff dispatch — route the five HandoffType variants onto the Phase K
//! layered substrates.
//!
//! Per COOPERATION.md §20 handoff patterns map onto Phase K layers:
//! - `Direct`               → L3 direct inbox (`routing::direct::DirectInbox`)
//! - `EnvironmentMediated`  → L0/shared workspace (`state::Workspace`)
//! - `FlexibleChain`        → L1 capability-indexed queue (`routing::index::CapabilityIndex`)
//! - `SequentialDependency` → logical obligation (no substrate; tracked by caller)
//! - `InformationTransfer`  → L2 state broadcast (callers attach a `StateMessage`)

use crate::action::SubTask;
use crate::cadence::AgentId;
use crate::routing::direct::DirectInbox;
use crate::routing::index::CapabilityIndex;
use crate::routing::types::{PriorityTask, TaskId};
use crate::state::Workspace;

use super::HandoffType;

#[derive(Debug)]
pub enum HandoffResult {
    /// L3 direct handoff delivered to receiver's inbox.
    Delivered { from: AgentId, to: AgentId, task_id: TaskId },
    /// L0/workspace deposit — receiver collects when ready.
    Deposited { location: String },
    /// L1 chain step posted to the capability-indexed queue.
    Queued { capability: String, step: usize, total_steps: usize, task_id: TaskId },
    /// Logical obligation registered (caller tracks fulfilment).
    ObligationRegistered {
        enabler: AgentId,
        enabled: AgentId,
        obligation: crate::cadence::ActionDescriptor,
    },
    /// L2 information transfer noted — recipients notified via bus elsewhere.
    Informed { recipients: Vec<AgentId> },
    /// Handoff could not be performed.
    Failed(String),
}

/// Dispatch a handoff onto the appropriate Phase K substrate.
///
/// The caller provides the substrates it has — a Workspace for
/// environment-mediated deposits, a DirectInbox for direct handoffs, a
/// CapabilityIndex for flexible chain steps. When a substrate is not
/// supplied for a variant that needs one, the handoff is `Failed`. This
/// keeps the function composable: bots without a routing layer still get
/// informational/obligation handoffs.
pub fn dispatch_handoff(
    handoff: &HandoffType,
    workspace: Option<&dyn Workspace>,
    inbox: Option<&DirectInbox>,
    chain_index: Option<&CapabilityIndex>,
) -> HandoffResult {
    match handoff {
        HandoffType::Direct {
            sender,
            receiver,
            payload,
        } => {
            let Some(inbox) = inbox else {
                return HandoffResult::Failed(
                    "direct handoff needs DirectInbox substrate".into(),
                );
            };
            let task = subtask_from_payload(*sender, *receiver, &payload.schema, &payload.data);
            let task_id = task.id;
            inbox.push(*receiver, task_id);
            HandoffResult::Delivered {
                from: *sender,
                to: *receiver,
                task_id,
            }
        }

        HandoffType::EnvironmentMediated {
            depositor,
            deposit_location,
            payload,
            ..
        } => {
            let Some(workspace) = workspace else {
                return HandoffResult::Failed(
                    "environment-mediated handoff needs Workspace substrate".into(),
                );
            };
            workspace.write(deposit_location.clone(), payload.data.clone(), *depositor);
            HandoffResult::Deposited {
                location: deposit_location.to_string(),
            }
        }

        HandoffType::FlexibleChain {
            originator,
            current_step,
            total_steps,
            payload,
            next_capability_required,
        } => {
            let Some(index) = chain_index else {
                return HandoffResult::Failed(
                    "flexible chain handoff needs CapabilityIndex substrate".into(),
                );
            };
            let task = subtask_for_chain(
                *originator,
                next_capability_required.clone(),
                &payload.data,
                *current_step + 1,
            );
            let task_id = task.id;
            let capability = task.target_connector.name.clone();
            index.insert(PriorityTask::new(task));
            HandoffResult::Queued {
                capability,
                step: *current_step + 1,
                total_steps: *total_steps,
                task_id,
            }
        }

        HandoffType::SequentialDependency {
            enabler,
            enabled,
            return_obligation,
        } => HandoffResult::ObligationRegistered {
            enabler: *enabler,
            enabled: *enabled,
            obligation: return_obligation.clone(),
        },

        HandoffType::InformationTransfer {
            recipients,
            intelligence,
            ..
        } => {
            if recipients.is_empty() {
                return HandoffResult::Failed(
                    "information transfer needs at least one recipient".into(),
                );
            }
            tracing::debug!(
                intel_len = intelligence.len(),
                recipients = recipients.len(),
                "information handoff"
            );
            HandoffResult::Informed {
                recipients: recipients.clone(),
            }
        }
    }
}

/// Turn a direct-handoff payload into a concrete SubTask addressed to the
/// receiver. Kept local to this file because the shape is specific to how
/// handoff semantics are lowered onto the routing substrate.
fn subtask_from_payload(
    sender: AgentId,
    receiver: AgentId,
    schema: &str,
    data: &serde_json::Value,
) -> SubTask {
    SubTask {
        id: uuid::Uuid::new_v4(),
        target_connector: crate::capability::CapabilityDecl::new(schema),
        action_name: "handoff".to_owned(),
        params: data.clone(),
        priority: 1,
        assigned_to: Some(receiver),
        description: format!("direct handoff from {}", sender.0),
    }
}

fn subtask_for_chain(
    originator: AgentId,
    capability: crate::capability::CapabilityDecl,
    data: &serde_json::Value,
    step: usize,
) -> SubTask {
    SubTask {
        id: uuid::Uuid::new_v4(),
        target_connector: capability,
        action_name: "chain_step".to_owned(),
        params: data.clone(),
        priority: 3,
        assigned_to: None,
        description: format!("chain step {step} from {}", originator.0),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::handoff::HandoffPayload;
    use crate::state::InMemoryWorkspace;

    fn make_payload(schema: &str, consumable: &[&str]) -> HandoffPayload {
        use crate::capability::CapabilityDecl;
        HandoffPayload {
            data: serde_json::json!({"test": true}),
            schema: schema.to_owned(),
            produced_by: crate::cadence::ActionDescriptor {
                kind: "test_agent".to_owned(),
                target: None,
                payload_hash: 0,
            },
            consumable_by: consumable.iter().map(|s| CapabilityDecl::new(*s)).collect(),
            expires: None,
        }
    }

    #[test]
    fn direct_handoff_delivers_to_inbox() {
        let inbox = DirectInbox::new();
        let sender = AgentId::new();
        let receiver = AgentId::new();
        let handoff = HandoffType::Direct {
            sender,
            receiver,
            payload: make_payload("slack", &["slack"]),
        };
        let result = dispatch_handoff(&handoff, None, Some(&inbox), None);
        assert!(matches!(result, HandoffResult::Delivered { .. }));
        assert_eq!(inbox.len(receiver), 1);
    }

    #[test]
    fn direct_handoff_without_inbox_fails() {
        let handoff = HandoffType::Direct {
            sender: AgentId::new(),
            receiver: AgentId::new(),
            payload: make_payload("slack", &[]),
        };
        let result = dispatch_handoff(&handoff, None, None, None);
        assert!(matches!(result, HandoffResult::Failed(_)));
    }

    #[test]
    fn environment_mediated_writes_workspace() {
        let ws = InMemoryWorkspace::new();
        let depositor = AgentId::new();
        let handoff = HandoffType::EnvironmentMediated {
            depositor,
            deposit_location: "shared:result".into(),
            payload: make_payload("data", &[]),
            transform_required: None,
        };
        let result = dispatch_handoff(&handoff, Some(&ws), None, None);
        assert!(matches!(result, HandoffResult::Deposited { .. }));
        assert!(ws.read("shared:result").is_some());
    }

    #[test]
    fn flexible_chain_posts_to_capability_index() {
        let idx = CapabilityIndex::new();
        let handoff = HandoffType::FlexibleChain {
            originator: AgentId::new(),
            current_step: 0,
            total_steps: 3,
            payload: make_payload("data", &[]),
            next_capability_required: "github".into(),
        };
        let result = dispatch_handoff(&handoff, None, None, Some(&idx));
        assert!(matches!(
            result,
            HandoffResult::Queued {
                step: 1,
                total_steps: 3,
                ..
            }
        ));
        assert_eq!(idx.len(), 1);
        let peeked = idx.peek_best(&["github".into()]);
        assert!(peeked.is_some());
    }

    #[test]
    fn flexible_chain_without_index_fails() {
        let handoff = HandoffType::FlexibleChain {
            originator: AgentId::new(),
            current_step: 0,
            total_steps: 2,
            payload: make_payload("data", &[]),
            next_capability_required: "rare".into(),
        };
        let result = dispatch_handoff(&handoff, None, None, None);
        assert!(matches!(result, HandoffResult::Failed(_)));
    }

    #[test]
    fn sequential_dependency_registers_obligation() {
        let handoff = HandoffType::SequentialDependency {
            enabler: AgentId::new(),
            enabled: AgentId::new(),
            return_obligation: crate::cadence::ActionDescriptor {
                kind: "notify_completion".to_owned(),
                target: None,
                payload_hash: 0,
            },
        };
        let result = dispatch_handoff(&handoff, None, None, None);
        assert!(matches!(result, HandoffResult::ObligationRegistered { .. }));
    }

    #[test]
    fn information_transfer_needs_recipients() {
        let handoff = HandoffType::InformationTransfer {
            source: AgentId::new(),
            recipients: vec![],
            intelligence: "nothing".into(),
            perishable: false,
        };
        let result = dispatch_handoff(&handoff, None, None, None);
        assert!(matches!(result, HandoffResult::Failed(_)));
    }

    #[test]
    fn information_transfer_informs_recipients() {
        let source = AgentId::new();
        let recipient = AgentId::new();
        let handoff = HandoffType::InformationTransfer {
            source,
            recipients: vec![recipient],
            intelligence: "update".into(),
            perishable: false,
        };
        let result = dispatch_handoff(&handoff, None, None, None);
        match result {
            HandoffResult::Informed { recipients } => assert_eq!(recipients.len(), 1),
            _ => panic!("expected Informed"),
        }
    }
}
