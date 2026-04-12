//! Formation operations — create, deploy, pause, resume, dissolve, list.
//!
//! Uses the cooperation module directly. Formations are the user-facing
//! abstraction over the cooperative agent architecture (COOPERATION.pdf).

use serde::{Deserialize, Serialize};

use springtale_core::rule::{Action, Rule, RuleId, RuleStatus, RuleVersion, Trigger};

use crate::error::OperationError;
use crate::state::RuntimeState;

/// Formation info for listing.
#[derive(Debug, Serialize)]
pub struct FormationInfo {
    pub id: String,
    pub name: String,
    pub intent: String,
    pub status: String,
    pub member_count: usize,
    /// Connector names of formation members.
    pub members: Vec<String>,
    /// Real momentum tier from runtime — "Cold", "Warming", "Hot", "Fever".
    /// Persisted via config store key `momentum:{formation_id}`.
    pub momentum_tier: String,
}

/// Create a new formation — stores config, creates member entries.
pub async fn create_formation(
    state: &RuntimeState,
    name: String,
    intent: String,
    connectors: Vec<String>,
) -> Result<String, OperationError> {
    let formation_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now();

    let row = springtale_store::FormationRow {
        id: formation_id.clone(),
        name,
        intent,
        status: "draft".to_owned(),
        created_at: now,
        updated_at: now,
    };

    state.store.insert_formation(&row).await?;

    for connector in &connectors {
        let member = springtale_store::FormationMemberRow {
            id: uuid::Uuid::new_v4().to_string(),
            formation_id: formation_id.clone(),
            connector_name: connector.clone(),
            role_hint: None,
        };
        state.store.insert_formation_member(&member).await?;
    }

    Ok(formation_id)
}

/// Deploy a formation — changes status to active.
pub async fn deploy_formation(state: &RuntimeState, id: &str) -> Result<(), OperationError> {
    state.store.update_formation_status(id, "active").await?;
    Ok(())
}

/// Pause a formation.
pub async fn pause_formation(state: &RuntimeState, id: &str) -> Result<(), OperationError> {
    state.store.update_formation_status(id, "paused").await?;
    Ok(())
}

/// Resume a paused formation.
pub async fn resume_formation(state: &RuntimeState, id: &str) -> Result<(), OperationError> {
    state.store.update_formation_status(id, "active").await?;
    Ok(())
}

/// Dissolve a formation — removes formation and all its members.
pub async fn dissolve_formation(state: &RuntimeState, id: &str) -> Result<(), OperationError> {
    state.store.delete_formation(id).await?;
    Ok(())
}

/// Update a formation's intent.
pub async fn update_intent(
    state: &RuntimeState,
    id: &str,
    intent: &str,
) -> Result<(), OperationError> {
    state.store.update_formation_intent(id, intent).await?;
    Ok(())
}

/// Add a member (connector) to a formation.
pub async fn add_member(
    state: &RuntimeState,
    formation_id: &str,
    connector_name: &str,
) -> Result<(), OperationError> {
    let member = springtale_store::FormationMemberRow {
        id: uuid::Uuid::new_v4().to_string(),
        formation_id: formation_id.to_owned(),
        connector_name: connector_name.to_owned(),
        role_hint: None,
    };
    state.store.insert_formation_member(&member).await?;
    Ok(())
}

/// List all formations with member counts.
pub async fn list_formations(state: &RuntimeState) -> Result<Vec<FormationInfo>, OperationError> {
    let formations = state.store.list_formations().await?;
    let mut infos = Vec::new();

    for f in formations {
        let members = state.store.list_formation_members(&f.id).await?;
        let member_names: Vec<String> = members.iter().map(|m| m.connector_name.clone()).collect();

        // Read persisted momentum tier from config store (written by bot event loop)
        let momentum_key = format!("momentum:{}", f.id);
        let momentum_tier = super::config::get_config(&*state.store, &momentum_key)
            .await
            .ok()
            .and_then(|v| serde_json::from_value::<String>(v).ok())
            .unwrap_or_else(|| "Cold".to_owned());

        infos.push(FormationInfo {
            id: f.id,
            name: f.name,
            intent: f.intent,
            status: f.status,
            member_count: member_names.len(),
            members: member_names,
            momentum_tier,
        });
    }

    Ok(infos)
}

// ── Team deploy (atomic OOBE operation) ─────────────────────────────────────

/// Single agent slot in a team deploy request.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeamAgentSlot {
    pub connector_name: String,
    pub trigger_name: String,
    pub action_connector: String,
    pub action_name: String,
}

/// Request to deploy a complete team — creates rules + formation atomically.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeamDeployRequest {
    pub name: String,
    pub intent: String,
    pub agents: Vec<TeamAgentSlot>,
    pub guard_mode: bool,
}

/// Result of a team deploy.
#[derive(Debug, Serialize)]
pub struct TeamDeployResult {
    pub formation_id: String,
    pub rule_ids: Vec<String>,
}

/// Deploy a complete team — creates rules, formation, and marks onboarding done.
///
/// Atomic: if any rule fails to create, all previously created rules are
/// rolled back. This replaces the multi-step frontend loop that made N+3
/// IPC calls.
pub async fn deploy_team(
    state: &RuntimeState,
    team: TeamDeployRequest,
) -> Result<TeamDeployResult, OperationError> {
    if team.name.trim().is_empty() {
        return Err(OperationError::Validation("team name is required".into()));
    }
    if team.agents.is_empty() {
        return Err(OperationError::Validation(
            "team must have at least one agent".into(),
        ));
    }

    // 1. Create rules (with rollback on failure)
    let mut created_rule_ids: Vec<RuleId> = Vec::new();

    for agent in &team.agents {
        if agent.connector_name.is_empty() || agent.trigger_name.is_empty() {
            continue;
        }

        let mut actions = Vec::new();
        if !agent.action_name.is_empty() {
            actions.push(Action::RunConnector {
                connector: agent.action_connector.clone(),
                action: agent.action_name.clone(),
                params: serde_json::Map::new(),
            });
        }

        let rule = Rule {
            id: RuleId::new(),
            name: format!("{} — {}", team.name, agent.trigger_name),
            description: String::new(),
            status: RuleStatus::Enabled,
            version: RuleVersion(1),
            trigger: Trigger::ConnectorEvent {
                connector: agent.connector_name.clone(),
                event: agent.trigger_name.clone(),
            },
            conditions: Vec::new(),
            actions,
        };

        match super::rules::create_rule(state, rule).await {
            Ok(id) => created_rule_ids.push(id),
            Err(e) => {
                // Rollback all previously created rules
                for rid in &created_rule_ids {
                    let _ = super::rules::delete_rule(state, rid).await;
                }
                return Err(OperationError::Rule(format!(
                    "failed to create rule for {}: {e}",
                    agent.trigger_name
                )));
            }
        }
    }

    // 2. Derive unique connector names
    let connector_names: Vec<String> = {
        let mut seen = std::collections::HashSet::new();
        team.agents
            .iter()
            .filter(|a| !a.connector_name.is_empty())
            .filter_map(|a| {
                if seen.insert(a.connector_name.clone()) {
                    Some(a.connector_name.clone())
                } else {
                    None
                }
            })
            .collect()
    };

    // 3. Create and deploy formation
    let formation_id = create_formation(state, team.name, team.intent, connector_names).await?;
    deploy_formation(state, &formation_id).await?;

    // 4. Mark onboarding complete
    super::config::set_config(
        &*state.store,
        "onboarding:completed",
        serde_json::Value::Bool(true),
    )
    .await?;

    Ok(TeamDeployResult {
        formation_id,
        rule_ids: created_rule_ids.iter().map(|id| id.to_string()).collect(),
    })
}

// ── Intent cycling ──────────────────────────────────────────────────────────

/// Valid intent progression order.
const INTENTS: &[&str] = &["Reconnoiter", "Execute", "Stabilize", "Surge"];

/// Intent metadata for frontend display.
#[derive(Debug, Serialize)]
pub struct IntentInfo {
    pub value: String,
    pub label: String,
}

/// List valid formation intents with display labels.
///
/// Backend is the single source of truth for valid intents.
/// Frontend renders these dynamically instead of hardcoding.
pub fn list_intents() -> Vec<IntentInfo> {
    INTENTS
        .iter()
        .map(|i| IntentInfo {
            value: i.to_string(),
            label: match *i {
                "Reconnoiter" => "Reconnoiter — monitor, read-only",
                "Execute" => "Execute — take action",
                "Stabilize" => "Stabilize — maintain state",
                "Surge" => "Surge — maximum effort",
                other => other,
            }
            .to_string(),
        })
        .collect()
}

/// Cycle a formation's intent to the next in the progression.
///
/// Reconnoiter → Execute → Stabilize → Surge → Reconnoiter.
pub async fn cycle_intent(
    state: &RuntimeState,
    formation_id: &str,
) -> Result<String, OperationError> {
    let formations = state.store.list_formations().await?;
    let formation = formations
        .iter()
        .find(|f| f.id == formation_id)
        .ok_or_else(|| OperationError::NotFound(format!("formation {formation_id}")))?;

    let current_idx = INTENTS
        .iter()
        .position(|i| i.eq_ignore_ascii_case(&formation.intent))
        .unwrap_or(0);
    let next = INTENTS[(current_idx + 1) % INTENTS.len()];

    state
        .store
        .update_formation_intent(formation_id, next)
        .await?;

    Ok(next.to_owned())
}

// ── Formation autonomy cycling ──────────────────────────────────────────────

/// Cycle a formation's autonomy to the next level.
///
/// observe → suggest → act-with-approval → act-autonomously → observe.
pub async fn cycle_autonomy(
    state: &RuntimeState,
    formation_id: &str,
) -> Result<String, OperationError> {
    let key = format!("formation:{formation_id}");
    let current = super::agent::get_autonomy(&*state.store, &key).await?;

    let levels = [
        "observe",
        "suggest",
        "act-with-approval",
        "act-autonomously",
    ];
    let idx = levels.iter().position(|l| *l == current).unwrap_or(0);
    let next = levels[(idx + 1) % levels.len()];

    super::agent::set_autonomy(&*state.store, &key, next).await?;

    Ok(next.to_owned())
}
