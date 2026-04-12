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
use springtale_store::StorageBackend;

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
    _store: &Arc<dyn StorageBackend>,
    registry: &Arc<RwLock<ConnectorRegistry>>,
) -> Result<Vec<SubTask>, BotError> {
    let orchestrator = formation
        .orchestrator
        .as_ref()
        .ok_or_else(|| BotError::NotInitialized("no orchestrator adapter".into()))?;

    // Build member capability summary for the AI
    let member_summary = build_member_summary(formation, registry).await;

    // Build the orchestration prompt
    let system_prompt = format!(
        "You are a formation orchestrator for Springtale, a privacy-preserving automation platform.\n\
         Your formation's intent is: {:?}\n\
         Momentum tier: {:?}\n\
         Members ({} operational):\n{}\n\n\
         Decompose the intent into concrete subtasks. Each subtask targets a specific connector action.\n\
         Respond with a JSON array of subtasks:\n\
         ```json\n\
         [\n\
           {{\n\
             \"target_connector\": \"connector-name\",\n\
             \"action_name\": \"action-name\",\n\
             \"params\": {{}},\n\
             \"priority\": 1,\n\
             \"description\": \"what this subtask does\"\n\
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

        let caps = member.capabilities.join(", ");
        let role = format!("{:?}", member.current_role);

        // Look up connector capabilities from registry
        let connector_actions: Vec<String> = member
            .capabilities
            .iter()
            .filter_map(|cap| {
                // Capabilities may include connector names
                let entry = registry.get(cap)?;
                let actions: Vec<String> = entry
                    .host
                    .actions()
                    .iter()
                    .map(|a| format!("{}:{}", cap, a.name))
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
        .flat_map(|m| m.capabilities.iter().map(|c| c.as_str()))
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

        subtasks.push(SubTask {
            id: Uuid::new_v4(),
            target_connector: target,
            action_name,
            params,
            priority,
            assigned_to: None,
            description,
        });
    }

    Ok(subtasks)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::cooperation::cadence::{AgentId, IntentPattern};
    use crate::cooperation::formation::{FormationConstraints, FormationMember};
    use crate::orchestrator::fuel::FuelBudget;

    fn test_formation() -> Formation {
        Formation::new(
            vec![
                FormationMember::new(AgentId::new(), vec!["connector-github".to_owned()]),
                FormationMember::new(AgentId::new(), vec!["connector-telegram".to_owned()]),
            ],
            IntentPattern::Execute { plan_id: None },
            FormationConstraints::default(),
            FuelBudget::new(1_000_000),
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
        assert_eq!(subtasks[0].target_connector, "connector-github");
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
