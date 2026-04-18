//! Task dispatch — bridge between cooperation SubTask and core Action.
//!
//! Converts a SubTask (from the cooperative blackboard) into a core
//! Action (for dispatch_action). This is the bridge between the
//! game AI world (SubTask with connector + action_name + params)
//! and the bot infrastructure world (Action enum that the runtime
//! dispatch system understands).
//!
//! Per the plan: SubTask → Action::RunConnector → dispatch_action()
//! → sentinel evaluation → connector execution → result.

use springtale_cooperation::action::SubTask;
use springtale_cooperation::cadence::ActionDescriptor;

/// Convert a SubTask (from blackboard) into a core Action (for dispatch).
pub fn subtask_to_action(task: &SubTask) -> springtale_core::rule::action::Action {
    springtale_core::rule::action::Action::RunConnector {
        connector: task.target_connector.name.clone(),
        action: task.action_name.clone(),
        params: match &task.params {
            serde_json::Value::Object(map) => map.clone(),
            _ => serde_json::Map::new(),
        },
    }
}

/// Build an ActionDescriptor from a SubTask for tick reporting.
///
/// The ActionDescriptor captures what action was taken, enabling
/// interference detection to compare actions by kind + target.
pub fn subtask_to_descriptor(task: &SubTask) -> ActionDescriptor {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    task.params.to_string().hash(&mut hasher);

    ActionDescriptor {
        kind: task.action_name.clone(),
        target: Some(task.target_connector.name.clone()),
        payload_hash: hasher.finish(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn make_task() -> SubTask {
        SubTask {
            id: uuid::Uuid::new_v4(),
            target_connector: springtale_cooperation::capability::CapabilityDecl::new("connector-github"),
            action_name: "create_issue".to_owned(),
            params: serde_json::json!({"title": "test", "body": "hello"}),
            priority: 1,
            assigned_to: None,
            description: "test task".to_owned(),
        }
    }

    #[test]
    fn test_subtask_to_action() {
        let task = make_task();
        let action = subtask_to_action(&task);
        match action {
            springtale_core::rule::action::Action::RunConnector {
                connector,
                action,
                params,
            } => {
                assert_eq!(connector, "connector-github");
                assert_eq!(action, "create_issue");
                assert_eq!(params.get("title").and_then(|v| v.as_str()), Some("test"));
            }
            _ => panic!("expected RunConnector"),
        }
    }

    #[test]
    fn test_subtask_to_descriptor() {
        let task = make_task();
        let desc = subtask_to_descriptor(&task);
        assert_eq!(desc.kind, "create_issue");
        assert_eq!(desc.target, Some("connector-github".to_owned()));
        assert!(desc.payload_hash != 0);
    }

    #[test]
    fn test_different_params_different_hash() {
        let mut task1 = make_task();
        let mut task2 = make_task();
        task1.params = serde_json::json!({"a": 1});
        task2.params = serde_json::json!({"b": 2});
        let d1 = subtask_to_descriptor(&task1);
        let d2 = subtask_to_descriptor(&task2);
        assert_ne!(d1.payload_hash, d2.payload_hash);
    }
}
