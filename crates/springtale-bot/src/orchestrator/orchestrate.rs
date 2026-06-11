//! Formation orchestration — AI-driven intent decomposition.
//!
//! Per COOPERATION.pdf §3: the orchestrator owns composition, intent,
//! constraints, and intervention. This module implements the intent
//! decomposition step: given a formation's intent and member capabilities,
//! the AI proposes subtasks posted to the CooperativeBlackboard.
//!
//! Design informed by:
//! - CrewAI manager agent pattern (separate orchestrator with own LLM)
//! - Patapon Fever mechanic (AI only available at Fever momentum)
//! - L4D AI Director (adjusts pressure based on group health, never puppets)
//! - RimWorld work priorities (agents pull tasks, not push)

use std::sync::Arc;

use tokio::sync::RwLock;
use uuid::Uuid;

use springtale_ai::adapter::{AiOptions, AiRequest, ChatMessage};
use springtale_connector::registry::store::ConnectorRegistry;

use crate::cooperation::action::SubTask;
use crate::cooperation::formation::Formation;
use crate::error::BotError;

/// Orchestrate a formation tick — decompose intent into subtasks via AI.
///
/// Called from `handle_cadence_tick()` when `formation.can_orchestrate()`
/// returns true (AI adapter present + Fever momentum).
///
/// The AI receives:
/// - Formation intent (Reconnoiter/Execute/Stabilize/Surge)
/// - Member capabilities and connector names
/// - Current momentum tier and formation health
///
/// The AI returns structured subtask proposals in JSON. These are
/// parsed and posted to the blackboard for members to pull.
pub async fn orchestrate_formation(
    formation: &Formation,
    registry: &Arc<RwLock<ConnectorRegistry>>,
) -> Result<Vec<SubTask>, BotError> {
    // AI orchestration is the augmentation path: it only runs when an adapter
    // is attached AND the formation has earned Fever momentum. Without it we
    // fall back to deterministic decomposition so that a formation with
    // `NoopAdapter` (the product-model default) still produces outward work.
    // This is the "AI is optional augmentation" invariant: everything works
    // without AI, AI makes it better.
    let orchestrator = match formation.orchestrator.as_ref() {
        Some(adapter) if formation.momentum.can_ai_orchestrate() => adapter,
        _ => return Ok(decompose_intent_deterministic(formation, registry).await),
    };

    // Build member capability summary for the AI
    let member_summary = build_member_summary(formation, registry).await;

    // Build the orchestration prompt
    let system_prompt = format!(
        "You are a formation orchestrator for Springtale, a privacy-preserving automation platform.\n\
         Your formation's intent is: {:?}\n\
         Momentum tier: {:?}\n\
         Members ({} operational):\n{}\n\n\
         Decompose the intent into concrete subtasks. Each subtask targets a specific connector action.\n\
         Subtasks may form a pipeline: give a subtask an `id` (any unique string), list upstream ids in\n\
         `depends_on`, and reference upstream output in params as \"${{result:<id>}}\" or\n\
         \"${{result:<id>.field.path}}\" — dependent tasks run only after their upstreams complete.\n\
         Respond with a JSON array of subtasks:\n\
         ```json\n\
         [\n\
           {{\n\
             \"id\": \"t1\",\n\
             \"target_connector\": \"connector-name\",\n\
             \"action_name\": \"action-name\",\n\
             \"params\": {{}},\n\
             \"priority\": 1,\n\
             \"description\": \"what this subtask does\",\n\
             \"depends_on\": []\n\
           }}\n\
         ]\n\
         ```\n\
         Only propose actions that the available connectors support.\n\
         Respond ONLY with the JSON array, no other text.",
        formation.intent,
        formation.momentum.tier,
        formation.operational_count(),
        member_summary,
    );

    let request = AiRequest::Chat {
        messages: vec![
            ChatMessage::text("system", system_prompt),
            ChatMessage::text(
                "user",
                format!(
                    "Generate subtasks for intent {:?} with {} members.",
                    formation.intent,
                    formation.operational_count(),
                ),
            ),
        ],
    };

    let response = orchestrator.complete(request, AiOptions::default()).await?;

    // Parse the AI response into subtasks
    parse_subtasks(&response.content, formation)
}

/// Build a summary of member capabilities for the AI context.
async fn build_member_summary(
    formation: &Formation,
    registry: &Arc<RwLock<ConnectorRegistry>>,
) -> String {
    let registry = registry.read().await;
    let mut lines = Vec::new();

    for member in &formation.members {
        if !member.is_operational() {
            continue;
        }

        let cap_names: Vec<&str> = member
            .capabilities
            .iter()
            .map(|c| c.name.as_str())
            .collect();
        let caps = cap_names.join(", ");
        let role = member.role.name().to_owned();

        // Look up connector capabilities from registry
        let connector_actions: Vec<String> = member
            .capabilities
            .iter()
            .filter_map(|cap| {
                let entry = registry.get(&cap.name)?;
                let actions: Vec<String> = entry
                    .host
                    .actions()
                    .iter()
                    .map(|a| format!("{}:{}", cap.name, a.name))
                    .collect();
                Some(actions)
            })
            .flatten()
            .collect();

        lines.push(format!(
            "- Agent {}: role={}, capabilities=[{}], available_actions=[{}]",
            member.agent_id.0,
            role,
            caps,
            connector_actions.join(", "),
        ));
    }

    lines.join("\n")
}

/// Parse the AI response content into structured subtasks.
fn parse_subtasks(content: &str, formation: &Formation) -> Result<Vec<SubTask>, BotError> {
    // Extract JSON from the response (may be wrapped in markdown code blocks).
    // Handles: bare JSON, ```json\n...\n```, or ```\n...\n```
    let trimmed = content.trim();
    let json_str = if trimmed.starts_with("```") {
        // Strip opening fence (```json or ```)
        let after_open = if let Some(rest) = trimmed.strip_prefix("```json") {
            rest
        } else if let Some(rest) = trimmed.strip_prefix("```") {
            rest
        } else {
            trimmed
        };
        // Strip closing fence
        let before_close = after_open
            .trim()
            .strip_suffix("```")
            .unwrap_or(after_open.trim());
        before_close.trim()
    } else {
        trimmed
    };

    let parsed: Vec<serde_json::Value> = serde_json::from_str(json_str)
        .map_err(|e| BotError::Handler(format!("failed to parse subtask JSON: {e}")))?;

    let mut subtasks = Vec::new();
    let member_connectors: Vec<&str> = formation
        .members
        .iter()
        .flat_map(|m| m.capabilities.iter().map(|c| c.name.as_str()))
        .collect();

    // W3 pass 1: assign every entry a real Uuid and map the AI's free-form
    // `id` strings ("t1") onto them so `depends_on` references resolve.
    let id_map: std::collections::HashMap<String, Uuid> = parsed
        .iter()
        .filter_map(|v| v.get("id").and_then(|s| s.as_str()))
        .map(|ai_id| (ai_id.to_owned(), Uuid::new_v4()))
        .collect();

    for val in parsed {
        let target = val
            .get("target_connector")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();

        // Only accept subtasks targeting connectors our members have
        if !member_connectors.contains(&target.as_str()) {
            tracing::warn!(
                target = %target,
                "orchestrator proposed subtask for connector not in formation — skipping"
            );
            continue;
        }

        let action_name = val
            .get("action_name")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();

        let params = val
            .get("params")
            .cloned()
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

        let priority = val.get("priority").and_then(|v| v.as_u64()).unwrap_or(5) as u8;

        let description = val
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        // W3: the squad AI may emit dependent-task DAGs. `depends_on`
        // entries are the AI's own `id` strings, mapped to real Uuids via
        // pass 1; raw UUIDs are accepted too. Unknown references are
        // dropped (the dep gate would otherwise block the task forever).
        let depends_on: Vec<Uuid> = val
            .get("depends_on")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|s| s.as_str())
                    .filter_map(|s| id_map.get(s).copied().or_else(|| Uuid::parse_str(s).ok()))
                    .collect()
            })
            .unwrap_or_default();

        // Rewrite `${result:t1...}` placeholders in params to the mapped
        // Uuids so the executor's resolver finds the blackboard entries.
        let mut params = params;
        rewrite_result_refs(&mut params, &id_map);

        let task_id = val
            .get("id")
            .and_then(|s| s.as_str())
            .and_then(|s| id_map.get(s).copied())
            .unwrap_or_else(Uuid::new_v4);

        subtasks.push(SubTask {
            id: task_id,
            target_connector: springtale_cooperation::capability::CapabilityDecl::new(target),
            action_name,
            params,
            priority,
            assigned_to: None,
            description,
            depends_on,
        });
    }

    Ok(subtasks)
}

/// Rewrite `${result:<ai-id>...}` placeholders to `${result:<uuid>...}`
/// using the pass-1 id map, so the executor's resolver matches the real
/// blackboard keys. Strings without a mapped prefix pass through unchanged.
fn rewrite_result_refs(
    value: &mut serde_json::Value,
    id_map: &std::collections::HashMap<String, Uuid>,
) {
    match value {
        serde_json::Value::String(s) if s.starts_with("${result:") && s.ends_with('}') => {
            let inner = &s["${result:".len()..s.len() - 1];
            let (id_part, path) = match inner.split_once('.') {
                Some((id, p)) => (id, Some(p)),
                None => (inner, None),
            };
            if let Some(uuid) = id_map.get(id_part) {
                *s = match path {
                    Some(p) => format!("${{result:{uuid}.{p}}}"),
                    None => format!("${{result:{uuid}}}"),
                };
            }
        }
        serde_json::Value::Object(map) => {
            for v in map.values_mut() {
                rewrite_result_refs(v, id_map);
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr.iter_mut() {
                rewrite_result_refs(v, id_map);
            }
        }
        _ => {}
    }
}

/// Deterministic (no-AI) intent decomposition.
///
/// Mirrors the AI path's output shape (`Vec<SubTask>` posted to the blackboard)
/// but derives subtasks mechanically from member connector capabilities + the
/// formation's `IntentPattern`, with zero LLM involvement. This is what gives a
/// `NoopAdapter` formation outward effect.
///
/// Scope, by design (honest bounds — the richer, parameterised work lives in
/// the event-driven formation rule synthesiser, `springtale-runtime`
/// `operations::formation_synthesis`, where params come from the trigger
/// payload rather than being invented):
///
/// - Only actions whose inputs are fully optional (no required params) are
///   emitted — we never fabricate parameters for an action that requires them.
/// - Under `Reconnoiter` (monitor, read-only) only actions the connector
///   declares as `read_only` (the MCP `readOnlyHint` on `ActionDecl`) are
///   emitted — precise read/poll selection rather than a name heuristic. Under
///   `Execute` / `Surge` any no-param action is fair game. `Stabilize` and
///   `Dissolve` emit nothing. `Surge` raises subtask priority.
/// - Subtask ids are stable (hash over agent+connector+action) so re-posting
///   the same poll each tick overwrites rather than accumulating.
/// - Gated by the momentum × layer authority matrix (L1 routine routing) so the
///   path respects the same tier discipline as the rest of the tick.
pub async fn decompose_intent_deterministic(
    formation: &Formation,
    registry: &Arc<RwLock<ConnectorRegistry>>,
) -> Vec<SubTask> {
    use crate::cooperation::IntentPattern;

    // (priority, restrict-to-read-only). Stabilize/Dissolve emit nothing.
    let (priority, read_only_only) = match &formation.intent {
        IntentPattern::Surge { .. } => (1, false), // max commitment → highest priority
        IntentPattern::Execute { .. } => (5, false),
        IntentPattern::Reconnoiter { .. } => (5, true), // monitor → reads only
        IntentPattern::Stabilize { .. } | IntentPattern::Dissolve { .. } => return Vec::new(),
    };

    // Read-poll subtasks are L1 routine routing — available at every tier, so
    // a Cold/Warming formation can still monitor. The mutating differentiation
    // is enforced downstream (synthesised rules + sentinel + autonomy gate).
    if !springtale_cooperation::authority::allows(
        formation.momentum.tier,
        springtale_cooperation::layer::LayerId::L1Routine,
    ) {
        return Vec::new();
    }

    let registry = registry.read().await;
    let mut subtasks = Vec::new();

    for member in &formation.members {
        if !member.is_operational() {
            continue;
        }
        for cap in &member.capabilities {
            let Some(entry) = registry.get(&cap.name) else {
                continue;
            };
            for decl in entry.host.actions() {
                if read_only_only && !decl.read_only {
                    continue; // Reconnoiter: monitor with read-only actions only
                }
                if action_requires_params(decl) {
                    continue; // we never invent required parameters
                }
                // Stable (deterministic) subtask id over agent+connector+action
                // so re-posting the same poll each tick overwrites rather than
                // accumulating on the blackboard.
                let stable = stable_task_id(member.agent_id.0, &cap.name, &decl.name);
                subtasks.push(SubTask {
                    id: stable,
                    target_connector: springtale_cooperation::capability::CapabilityDecl::new(
                        cap.name.clone(),
                    ),
                    action_name: decl.name.clone(),
                    params: serde_json::Value::Object(serde_json::Map::new()),
                    priority,
                    assigned_to: None,
                    description: format!(
                        "deterministic {:?} poll: {}:{}",
                        intent_label(&formation.intent),
                        cap.name,
                        decl.name
                    ),
                    // Deterministic polls are independent reads — no deps.
                    depends_on: Vec::new(),
                });
            }
        }
    }

    subtasks
}

/// Deterministic subtask id derived from agent + connector + action. Stable
/// within a running daemon so re-posts dedup on the blackboard. Uses a hash
/// (not UUIDv5) to avoid pulling in the uuid `v5` feature.
fn stable_task_id(agent: uuid::Uuid, connector: &str, action: &str) -> uuid::Uuid {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    (agent, connector, action).hash(&mut hasher);
    let hi = hasher.finish();
    action.hash(&mut hasher);
    let lo = hasher.finish();
    uuid::Uuid::from_u128(((hi as u128) << 64) | lo as u128)
}

/// Short label for an intent, used in subtask descriptions.
fn intent_label(intent: &crate::cooperation::IntentPattern) -> &'static str {
    use crate::cooperation::IntentPattern;
    match intent {
        IntentPattern::Reconnoiter { .. } => "Reconnoiter",
        IntentPattern::Execute { .. } => "Execute",
        IntentPattern::Stabilize { .. } => "Stabilize",
        IntentPattern::Surge { .. } => "Surge",
        IntentPattern::Dissolve { .. } => "Dissolve",
    }
}

/// Whether an action declares any *required* input parameters. We only emit
/// deterministic subtasks for actions that require none, so we never fabricate
/// argument values. An absent input schema means "no inputs" → no requirements.
fn action_requires_params(decl: &springtale_connector::manifest::types::ActionDecl) -> bool {
    match decl.input_schema.as_ref() {
        None => false,
        Some(schema) => schema
            .get("required")
            .and_then(|r| r.as_array())
            .is_some_and(|reqs| !reqs.is_empty()),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::cooperation::formation::FormationMember;
    use crate::cooperation::{AgentId, FormationConstraints, IntentPattern};

    fn test_formation() -> Formation {
        Formation::new_disconnected(
            vec![
                FormationMember::new(AgentId::new(), vec!["connector-github".into()]),
                FormationMember::new(AgentId::new(), vec!["connector-telegram".into()]),
            ],
            IntentPattern::Execute { plan_id: None },
            FormationConstraints {
                fuel_budget: springtale_cooperation::FuelAmount(1_000_000),
                ..Default::default()
            },
        )
    }

    #[test]
    fn test_parse_subtasks_valid_json() {
        let formation = test_formation();
        let json = r#"[
            {
                "target_connector": "connector-github",
                "action_name": "create_issue",
                "params": {"title": "test"},
                "priority": 1,
                "description": "Create a test issue"
            }
        ]"#;

        let subtasks = parse_subtasks(json, &formation).unwrap();
        assert_eq!(subtasks.len(), 1);
        assert_eq!(subtasks[0].target_connector, *"connector-github");
        assert_eq!(subtasks[0].action_name, "create_issue");
        assert_eq!(subtasks[0].priority, 1);
    }

    #[test]
    fn test_parse_subtasks_filters_unknown_connectors() {
        let formation = test_formation();
        let json = r#"[
            {
                "target_connector": "connector-unknown",
                "action_name": "do_something",
                "params": {},
                "priority": 1,
                "description": "Should be filtered"
            }
        ]"#;

        let subtasks = parse_subtasks(json, &formation).unwrap();
        assert_eq!(subtasks.len(), 0);
    }

    #[test]
    fn test_parse_subtasks_markdown_wrapped() {
        let formation = test_formation();
        let json = "```json\n[\n{\"target_connector\":\"connector-github\",\"action_name\":\"list_issues\",\"params\":{},\"priority\":1,\"description\":\"List issues\"}\n]\n```";

        let subtasks = parse_subtasks(json, &formation).unwrap();
        assert_eq!(subtasks.len(), 1);
    }
}
