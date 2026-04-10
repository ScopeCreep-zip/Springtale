//! Agent operations — autonomy, state aggregation.
//!
//! Autonomy levels stored via the bot alias table as a key-value store.
//! Agent state aggregates rules + recent events + autonomy into a single
//! response for the frontend to render without computing business logic.

use serde::Serialize;
use springtale_store::StorageBackend;
use springtale_store::schema::events::{EventEntry, EventFilter};

use crate::error::OperationError;
use crate::state::RuntimeState;

/// Valid autonomy levels (ARCHITECTURE.md §5.3).
const VALID_LEVELS: &[&str] = &[
    "observe",           // L0
    "suggest",           // L1
    "act-with-approval", // L2
    "act-autonomously",  // L3
];

/// Set the autonomy level for an agent.
///
/// Valid levels: "observe" (L0), "suggest" (L1), "act-with-approval" (L2),
/// "act-autonomously" (L3).
pub async fn set_autonomy(
    store: &dyn StorageBackend,
    agent_name: &str,
    level: &str,
) -> Result<(), OperationError> {
    if !VALID_LEVELS.contains(&level) {
        return Err(OperationError::Validation(format!(
            "invalid autonomy level '{level}': must be one of: {}",
            VALID_LEVELS.join(", ")
        )));
    }

    let alias_key = format!("autonomy:{agent_name}");
    store
        .upsert_alias(&alias_key, level, "cli")
        .await
        .map_err(OperationError::Store)?;
    Ok(())
}

/// Get the current autonomy level for an agent.
///
/// Returns "suggest" (L1) if no level has been set.
pub async fn get_autonomy(
    store: &dyn StorageBackend,
    agent_name: &str,
) -> Result<String, OperationError> {
    let alias_key = format!("autonomy:{agent_name}");
    let aliases = store.list_aliases().await.map_err(OperationError::Store)?;
    let level = aliases
        .iter()
        .find(|(k, _)| k == &alias_key)
        .map(|(_, v)| v.clone())
        .unwrap_or_else(|| "suggest".to_owned());
    Ok(level)
}

/// Step the autonomy level up or down.
///
/// Returns the new level string. If already at the boundary (L0 for down,
/// L3 for up), returns the current level unchanged.
pub async fn step_autonomy(
    store: &dyn StorageBackend,
    agent_name: &str,
    direction: AutonomyDirection,
) -> Result<String, OperationError> {
    let current = get_autonomy(store, agent_name).await?;
    let idx = VALID_LEVELS
        .iter()
        .position(|l| *l == current)
        .unwrap_or(1); // default to "suggest" index

    let new_idx = match direction {
        AutonomyDirection::Up => {
            if idx < VALID_LEVELS.len() - 1 { idx + 1 } else { idx }
        }
        AutonomyDirection::Down => {
            if idx > 0 { idx - 1 } else { idx }
        }
    };

    let new_level = VALID_LEVELS[new_idx];
    if new_idx != idx {
        set_autonomy(store, agent_name, new_level).await?;
    }

    Ok(new_level.to_owned())
}

/// Direction for stepping autonomy.
#[derive(Debug, Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AutonomyDirection {
    Up,
    Down,
}

// ── Agent state aggregation ─────────────────────────────────────────────────

/// Aggregated agent state — joins rule + recent events + autonomy.
///
/// This is what the frontend renders. All business logic (role inference,
/// activity computation, fuel derivation) lives here, not in the frontend.
#[derive(Debug, Serialize, Clone)]
pub struct AgentState {
    pub rule_id: String,
    pub name: String,
    pub status: String,
    pub trigger_type: String,
    pub connector_name: Option<String>,
    /// Agent role — inferred from trigger type semantics.
    pub role: String,
    /// Fuel: 100 when enabled, 0 when disabled.
    pub fuel: u8,
    /// Activity state derived from recent events.
    pub activity: String,
    /// Autonomy level index (0=observe, 1=suggest, 2=approve, 3=autonomous).
    pub autonomy: u8,
}

/// Infer an agent's role from its trigger type.
///
/// Moved from frontend `mappers.ts` — this is domain semantics
/// that belongs in the backend.
fn infer_role(trigger_type: &str) -> &'static str {
    let t = trigger_type.to_lowercase();
    if t.contains("command") || t.contains("message") {
        "guard"
    } else if t.contains("search") || t.contains("scrape") {
        "analyst"
    } else if t.contains("monitor") || t.contains("watch") || t.contains("stream") {
        "scout"
    } else {
        "worker"
    }
}

/// Compute agent activity from recent events.
///
/// Moved from frontend `ColonyCanvas.tsx` — event interpretation
/// belongs in the backend, not derived client-side.
fn compute_activity(
    connector_name: &Option<String>,
    trigger_type: &str,
    status: &str,
    events: &[EventEntry],
) -> &'static str {
    if status != "enabled" {
        return "idle";
    }

    // Find the most recent event matching this agent's connector or trigger
    let latest = events.iter().find(|e| {
        connector_name
            .as_ref()
            .map_or(false, |cn| e.connector_name == *cn)
            || e.trigger_type == trigger_type
    });

    let Some(event) = latest else {
        return "waiting";
    };

    let age_ms = (chrono::Utc::now() - event.timestamp).num_milliseconds();
    if age_ms < 5_000 {
        // Check for error indicators in the action text
        let action_lower = event.action_taken.to_lowercase();
        if action_lower.contains("error")
            || action_lower.contains("fail")
            || action_lower.contains("block")
        {
            return "error";
        }
        return "firing";
    }
    if age_ms < 60_000 {
        return "active";
    }
    "waiting"
}

/// Autonomy level string to index.
fn autonomy_to_index(level: &str) -> u8 {
    match level {
        "observe" => 0,
        "suggest" => 1,
        "act-with-approval" => 2,
        "act-autonomously" => 3,
        _ => 1, // default to suggest
    }
}

/// List aggregated agent states for all rules.
///
/// Joins rule data (from engine) with recent events (from store)
/// and autonomy levels (from alias table) into a single response
/// the frontend can render without computing any business logic.
pub async fn list_agent_states(
    state: &RuntimeState,
) -> Result<Vec<AgentState>, OperationError> {
    // Gather rules from engine
    let rules = super::rules::list_rules(state).await;

    // Fetch recent events (last 200) for activity computation
    let events = state
        .store
        .list_events(&EventFilter {
            limit: Some(200),
            ..Default::default()
        })
        .await
        .map_err(OperationError::Store)?;

    // Fetch all autonomy levels
    let aliases = state
        .store
        .list_aliases()
        .await
        .map_err(OperationError::Store)?;

    let agents = rules
        .iter()
        .map(|r| {
            let autonomy_key = format!("autonomy:{}", r.name);
            let autonomy_str = aliases
                .iter()
                .find(|(k, _)| k == &autonomy_key)
                .map(|(_, v)| v.as_str())
                .unwrap_or("suggest");

            AgentState {
                rule_id: r.id.clone(),
                name: r.name.clone(),
                status: r.status.clone(),
                trigger_type: r.trigger_type.clone(),
                connector_name: r.connector_name.clone(),
                role: infer_role(&r.trigger_type).to_owned(),
                fuel: if r.status == "enabled" { 100 } else { 0 },
                activity: compute_activity(
                    &r.connector_name,
                    &r.trigger_type,
                    &r.status,
                    &events,
                )
                .to_owned(),
                autonomy: autonomy_to_index(autonomy_str),
            }
        })
        .collect();

    Ok(agents)
}
