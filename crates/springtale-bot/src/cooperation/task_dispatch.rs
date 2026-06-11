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

/// W3 cross-agent data pipe: resolve `${result:<uuid>}` and
/// `${result:<uuid>.<json.path>}` placeholders in a task's params from its
/// dependencies' posted results. The scan-side dependency gate guarantees
/// every referenced result exists by the time the task is claimable; a
/// missing/unparsable reference resolves to JSON null rather than blocking.
pub fn resolve_result_params(
    task: &mut SubTask,
    blackboard: &dyn crate::cooperation::blackboard::trait_::Blackboard,
) {
    fn walk(
        value: &mut serde_json::Value,
        blackboard: &dyn crate::cooperation::blackboard::trait_::Blackboard,
    ) {
        match value {
            serde_json::Value::String(s) if s.starts_with("${result:") && s.ends_with('}') => {
                let inner = &s["${result:".len()..s.len() - 1];
                let (id_str, path) = match inner.split_once('.') {
                    Some((id, p)) => (id, Some(p)),
                    None => (inner, None),
                };
                let resolved = uuid::Uuid::parse_str(id_str)
                    .ok()
                    .and_then(|id| blackboard.read_result(id))
                    .map(|r| match path {
                        // Dotted path → JSON-pointer over the result output.
                        Some(p) => r
                            .output
                            .pointer(&format!("/{}", p.replace('.', "/")))
                            .cloned()
                            .unwrap_or(serde_json::Value::Null),
                        None => r.output,
                    })
                    .unwrap_or(serde_json::Value::Null);
                *value = resolved;
            }
            serde_json::Value::Object(map) => {
                for v in map.values_mut() {
                    walk(v, blackboard);
                }
            }
            serde_json::Value::Array(arr) => {
                for v in arr.iter_mut() {
                    walk(v, blackboard);
                }
            }
            _ => {}
        }
    }
    walk(&mut task.params, blackboard);
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
            target_connector: springtale_cooperation::capability::CapabilityDecl::new(
                "connector-github",
            ),
            action_name: "create_issue".to_owned(),
            params: serde_json::json!({"title": "test", "body": "hello"}),
            priority: 1,
            assigned_to: None,
            description: "test task".to_owned(),
            depends_on: Vec::new(),
        }
    }

    /// W3 end-to-end: researcher→writer over the blackboard. The writer is
    /// invisible until the researcher's result lands (dependency gate), then
    /// its `${result:...}` params materialize the researcher's output.
    #[test]
    fn dependent_task_gates_then_consumes_upstream_output() {
        use crate::cooperation::blackboard::cooperative::CooperativeBlackboard;
        use crate::cooperation::blackboard::trait_::Blackboard;
        use crate::orchestrator::fuel::FuelBudget;

        let bb = CooperativeBlackboard::new();
        let fuel = FuelBudget::new(1_000);

        let researcher = make_task(); // independent
        let mut writer = make_task();
        writer.depends_on = vec![researcher.id];
        writer.params = serde_json::json!({
            "text": format!("${{result:{}.notes}}", researcher.id),
            "whole": format!("${{result:{}}}", researcher.id),
        });

        for t in [&researcher, &writer] {
            bb.write(
                &format!("task:{}", t.id),
                serde_json::to_value(t).unwrap(),
                uuid::Uuid::new_v4(),
                &fuel,
            )
            .unwrap();
        }

        // Gate: only the researcher is claimable while its result is absent.
        let visible: Vec<_> = bb.scan_tasks(&[]).into_iter().map(|t| t.id).collect();
        assert!(visible.contains(&researcher.id));
        assert!(!visible.contains(&writer.id), "writer gated on unmet dep");

        // Researcher completes → result posted.
        bb.post_result(
            &springtale_cooperation::SubTaskResult {
                task_id: researcher.id,
                agent_id: springtale_cooperation::cadence::AgentId::new(),
                success: true,
                output: serde_json::json!({"notes": "three cited findings"}),
                duration_ms: 5,
            },
            &fuel,
        )
        .unwrap();

        // Gate lifts; resolution materializes the upstream output.
        let visible: Vec<_> = bb.scan_tasks(&[]).into_iter().map(|t| t.id).collect();
        assert!(
            visible.contains(&writer.id),
            "writer claimable after dep met"
        );

        let mut claimed = writer.clone();
        resolve_result_params(&mut claimed, &bb);
        assert_eq!(claimed.params["text"], "three cited findings");
        assert_eq!(claimed.params["whole"]["notes"], "three cited findings");
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
