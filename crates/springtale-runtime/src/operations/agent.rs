//! Agent operations — autonomy, state aggregation.
//!
//! Autonomy is one setting per level, stored in the config KV and keyed by
//! the thing it governs: `autonomy:agent:{rule_id}` for a single rule/agent,
//! `autonomy:formation:{id}` for a formation. Nothing is keyed by name, so a
//! rename orphans nothing. [`resolve_autonomy`] is the one read every
//! dispatch makes (ALIGNMENT-PLAN §6.2).
//!
//! Agent state aggregates rules + recent events + autonomy into a single
//! response for the frontend to render without computing business logic.

use serde::Serialize;
use specta::Type;
use springtale_cooperation::AutonomyLevel;
use springtale_core::rule::action::Action;
use springtale_core::rule::types::Rule;
use springtale_store::StorageBackend;
use springtale_store::schema::events::{EventEntry, EventFilter};

use crate::error::OperationError;
use crate::state::RuntimeState;

/// Valid autonomy level strings (ARCHITECTURE.md §5.3), lowest to highest.
const VALID_LEVELS: &[&str] = &[
    "observe",           // L0
    "suggest",           // L1
    "act-with-approval", // L2
    "act-autonomously",  // L3
];

/// What an autonomy setting governs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutonomyTarget {
    /// A formation, keyed by its id string.
    Formation { id: String },
    /// A single agent, keyed by the id of the rule it fires.
    Agent { rule_id: uuid::Uuid },
}

impl AutonomyTarget {
    /// Config-store key for this target.
    fn config_key(&self) -> String {
        match self {
            Self::Formation { id } => format!("autonomy:formation:{id}"),
            Self::Agent { rule_id } => format!("autonomy:agent:{rule_id}"),
        }
    }
}

/// Strict parse: `None` on unrecognized input. (`AutonomyLevel::parse`
/// falls back to `Suggest`, which would turn a corrupt row into a policy.)
fn parse_level_opt(s: &str) -> Option<AutonomyLevel> {
    VALID_LEVELS.contains(&s).then(|| AutonomyLevel::parse(s))
}

/// Read one config row as a level; `None` when unset, unreadable, or invalid.
async fn read_level(store: &dyn StorageBackend, key: &str) -> Option<AutonomyLevel> {
    match store.get_config(key).await {
        Ok(Some(raw)) => parse_level_opt(&raw),
        _ => None,
    }
}

/// Resolve the name-or-id an operator typed to an agent target.
///
/// A UUID is taken as the rule id directly; anything else is looked up as a
/// rule name in the engine. Only the id is ever written to the store.
pub async fn resolve_agent_target(
    state: &RuntimeState,
    name_or_id: &str,
) -> Result<AutonomyTarget, OperationError> {
    if let Ok(rule_id) = uuid::Uuid::parse_str(name_or_id) {
        return Ok(AutonomyTarget::Agent { rule_id });
    }
    let engine = state.engine.read().await;
    let rule_id = engine
        .list_rules()
        .into_iter()
        .find(|r| r.name == name_or_id)
        .map(|r| r.id.0)
        .ok_or_else(|| OperationError::NotFound(format!("rule '{name_or_id}' not found")))?;
    Ok(AutonomyTarget::Agent { rule_id })
}

/// Set the autonomy level for a target.
///
/// Valid levels: "observe" (L0), "suggest" (L1), "act-with-approval" (L2),
/// "act-autonomously" (L3).
pub async fn set_autonomy(
    store: &dyn StorageBackend,
    target: &AutonomyTarget,
    level: &str,
) -> Result<(), OperationError> {
    let level = parse_level_opt(level).ok_or_else(|| {
        OperationError::Validation(format!(
            "invalid autonomy level '{level}': must be one of: {}",
            VALID_LEVELS.join(", ")
        ))
    })?;
    store
        .set_config(&target.config_key(), level.as_str())
        .await
        .map_err(OperationError::Store)
}

/// Get the level set on a target. `ActAutonomously` when none has been set.
pub async fn get_autonomy(
    store: &dyn StorageBackend,
    target: &AutonomyTarget,
) -> Result<AutonomyLevel, OperationError> {
    let raw = store
        .get_config(&target.config_key())
        .await
        .map_err(OperationError::Store)?;
    Ok(raw
        .as_deref()
        .and_then(parse_level_opt)
        .unwrap_or(AutonomyLevel::ActAutonomously))
}

/// The one read every dispatch makes: the agent row wins, then the owning
/// formation's row, then `ActAutonomously`.
pub async fn resolve_autonomy(
    store: &dyn StorageBackend,
    rule_id: &uuid::Uuid,
    formation_id: Option<&str>,
) -> AutonomyLevel {
    let keys = [
        Some(AutonomyTarget::Agent { rule_id: *rule_id }.config_key()),
        formation_id.map(|id| AutonomyTarget::Formation { id: id.to_owned() }.config_key()),
    ];
    for key in keys.into_iter().flatten() {
        if let Some(level) = read_level(store, &key).await {
            return level;
        }
    }
    AutonomyLevel::ActAutonomously
}

/// Formation-level resolve for members that have no rule of their own yet
/// (synthesized formation rules are owned by the formation, not a member).
pub async fn resolve_formation_autonomy(
    store: &dyn StorageBackend,
    formation_id: &str,
) -> AutonomyLevel {
    let target = AutonomyTarget::Formation {
        id: formation_id.to_owned(),
    };
    read_level(store, &target.config_key())
        .await
        .unwrap_or(AutonomyLevel::ActAutonomously)
}

/// Step the autonomy level up or down.
///
/// Returns the new level string. If already at the boundary (L0 for down,
/// L3 for up), returns the current level unchanged.
pub async fn step_autonomy(
    store: &dyn StorageBackend,
    target: &AutonomyTarget,
    direction: AutonomyDirection,
) -> Result<String, OperationError> {
    let idx = usize::from(autonomy_to_index(get_autonomy(store, target).await?));

    let new_idx = match direction {
        AutonomyDirection::Up => (idx + 1).min(VALID_LEVELS.len() - 1),
        AutonomyDirection::Down => idx.saturating_sub(1),
    };

    let new_level = VALID_LEVELS
        .get(new_idx)
        .copied()
        .unwrap_or("act-autonomously");
    if new_idx != idx {
        set_autonomy(store, target, new_level).await?;
    }

    Ok(new_level.to_owned())
}

/// Direction for stepping autonomy.
#[derive(Debug, Clone, Copy, serde::Deserialize, Type, utoipa::ToSchema)]
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
#[derive(Debug, Serialize, Clone, Type)]
pub struct AgentState {
    pub rule_id: String,
    pub name: String,
    pub status: String,
    pub trigger_type: String,
    pub connector_name: Option<String>,
    /// Connector this agent *acts on* — the first [`Action::RunConnector`]
    /// target in its rule, or `None` when the rule only sends messages /
    /// writes files. Plan 3.5: the canvas walks the springtail part-way
    /// along the mycelium toward this tree while the rule is firing.
    pub action_connector: Option<String>,
    /// Agent role — inferred from trigger type semantics.
    pub role: String,
    /// Fuel: 100 when enabled, 0 when disabled.
    pub fuel: u8,
    /// Activity state derived from recent events.
    pub activity: String,
    /// Autonomy level index (0=observe, 1=suggest, 2=approve, 3=autonomous).
    pub autonomy: u8,
    /// Human label for autonomy level.
    pub autonomy_label: String,
    /// Fuel status derived from threshold: "ok" | "warn" | "critical".
    pub fuel_status: String,
    /// Pre-formatted task description for display.
    pub task_display: String,
    /// Attention load from formation's AttentionBroker (0.0–1.0).
    pub attention_load: f32,
    /// Liveness score (1.0 = alive, 0.0 = dead).
    pub liveness: f32,
    /// Health state: "healthy", "degraded", "incapacitated", "dead".
    pub health_state: String,
}

/// The connector a rule acts on — the first [`Action::RunConnector`] target.
///
/// A rule may chain several actions; the first connector call is the one the
/// springtail visibly walks toward, so that is the one reported.
fn action_connector(rule: &Rule) -> Option<String> {
    rule.actions.iter().find_map(|a| match a {
        Action::RunConnector { connector, .. } => Some(connector.clone()),
        _ => None,
    })
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
            .is_some_and(|cn| e.connector_name == *cn)
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

/// Autonomy level to its L0–L3 index.
fn autonomy_to_index(level: AutonomyLevel) -> u8 {
    match level {
        AutonomyLevel::Observe => 0,
        AutonomyLevel::Suggest => 1,
        AutonomyLevel::ActWithApproval => 2,
        AutonomyLevel::ActAutonomously => 3,
    }
}

/// Live formation member data — used to enrich AgentState with
/// real cooperation data when formations are active.
struct LiveAgentEnrichment {
    attention_load: f32,
    liveness: f32,
    health_state: String,
    /// Live fuel as a percentage of the member's initial budget.
    fuel_pct: u8,
}

/// Fuel remaining as a 0–100 percentage of the initial budget.
/// A zero initial budget reads as empty rather than dividing by zero.
fn fuel_pct(remaining: u64, initial: u64) -> u8 {
    if initial == 0 {
        return 0;
    }
    let pct = remaining.saturating_mul(100) / initial;
    u8::try_from(pct.min(100)).unwrap_or(100)
}

/// Fuel gauge label: `ok` above 50, `warn` above 20, `critical` otherwise.
fn fuel_status_label(fuel: u8) -> &'static str {
    if fuel > 50 {
        "ok"
    } else if fuel > 20 {
        "warn"
    } else {
        "critical"
    }
}

/// List aggregated agent states for all rules.
///
/// Joins rule data (from engine) with recent events (from store)
/// and autonomy levels (from the config store) into a single response
/// the frontend can render without computing any business logic.
///
/// When `state.live_formations` is available, cross-references agents
/// with live formation members to populate attention, liveness, and
/// health from real cooperation data.
pub async fn list_agent_states(state: &RuntimeState) -> Result<Vec<AgentState>, OperationError> {
    // Gather rules from engine
    let rules = super::rules::list_rules(state).await;

    // Action targets: rule id → the connector its first `RunConnector` action
    // calls. Read from the engine (the authoritative rule set) because
    // `RuleSummary` carries only trigger-side data.
    let action_targets: std::collections::HashMap<String, String> = {
        let engine = state.engine.read().await;
        engine
            .list_rules()
            .iter()
            .filter_map(|r| action_connector(r).map(|c| (r.id.to_string(), c)))
            .collect()
    };

    // Fetch recent events (last 200) for activity computation
    let events = state
        .store
        .list_events(&EventFilter {
            limit: Some(200),
            ..Default::default()
        })
        .await
        .map_err(OperationError::Store)?;

    // Fetch all config rows once; autonomy is keyed `autonomy:agent:{rule_id}`.
    let config = state
        .store
        .list_config()
        .await
        .map_err(OperationError::Store)?;

    // Build enrichment map from live formations when available.
    // Maps connector_name → live agent data.
    let mut enrichment_map: std::collections::HashMap<String, LiveAgentEnrichment> =
        std::collections::HashMap::new();
    if let Some(reader) = &state.live_formations {
        let formations = state.store.list_formations().await.unwrap_or_default();
        for f in &formations {
            let details = reader.read_member_details(&f.id).await;
            for detail in details {
                enrichment_map.insert(
                    detail.connector_name.clone(),
                    LiveAgentEnrichment {
                        attention_load: detail.attention_load,
                        liveness: match detail.liveness.as_str() {
                            "Alive" => 1.0,
                            "Suspect" => 0.5,
                            _ => 0.0,
                        },
                        health_state: detail.health.ui_state().to_owned(),
                        fuel_pct: fuel_pct(detail.fuel_remaining, detail.fuel_initial),
                    },
                );
            }
        }
    }

    let agents = rules
        .iter()
        .map(|r| {
            let autonomy_key = format!("autonomy:agent:{}", r.id);
            let autonomy = config
                .iter()
                .find(|(k, _)| k == &autonomy_key)
                .and_then(|(_, v)| parse_level_opt(v))
                .unwrap_or(AutonomyLevel::ActAutonomously);

            let activity = compute_activity(&r.connector_name, &r.trigger_type, &r.status, &events);
            let task_display = if activity == "idle" {
                "Idle".to_owned()
            } else {
                format!("{} → {}", r.trigger_type, activity)
            };

            let autonomy_idx = autonomy_to_index(autonomy);
            let autonomy_label = match autonomy_idx {
                0 => "OBSERVE",
                1 => "SUGGEST",
                2 => "APPROVE",
                _ => "AUTONOMOUS",
            }
            .to_owned();

            // Enrich from live formation data when available
            let enrichment = r
                .connector_name
                .as_ref()
                .and_then(|cn| enrichment_map.get(cn));

            // Live members report their real fuel budget; rules outside a
            // live formation have no budget, so enabled reads full and
            // disabled reads empty.
            let fuel: u8 = match enrichment {
                Some(e) => e.fuel_pct,
                None if r.status == "enabled" => 100,
                None => 0,
            };
            let fuel_status = fuel_status_label(fuel).to_owned();

            AgentState {
                rule_id: r.id.clone(),
                name: r.name.clone(),
                status: r.status.clone(),
                trigger_type: r.trigger_type.clone(),
                connector_name: r.connector_name.clone(),
                action_connector: action_targets.get(&r.id).cloned(),
                role: infer_role(&r.trigger_type).to_owned(),
                fuel,
                activity: activity.to_owned(),
                autonomy: autonomy_idx,
                autonomy_label,
                fuel_status,
                task_display,
                attention_load: enrichment.map(|e| e.attention_load).unwrap_or(0.0),
                liveness: enrichment.map(|e| e.liveness).unwrap_or(1.0),
                health_state: enrichment
                    .map(|e| e.health_state.clone())
                    .unwrap_or_else(|| "healthy".to_owned()),
            }
        })
        .collect();

    Ok(agents)
}

#[cfg(test)]
mod tests {
    use super::*;
    use springtale_store::backend::InMemoryBackend;

    #[tokio::test]
    async fn test_resolve_autonomy_prefers_agent_over_formation_and_defaults_autonomous() {
        let store = InMemoryBackend::new();
        let rule_id = uuid::Uuid::new_v4();
        let formation_id = uuid::Uuid::new_v4().to_string();

        // Nothing set: defaults to ActAutonomously.
        let level = resolve_autonomy(&store, &rule_id, Some(&formation_id)).await;
        assert_eq!(level, AutonomyLevel::ActAutonomously);

        // Formation row only: the member inherits it.
        let formation = AutonomyTarget::Formation {
            id: formation_id.clone(),
        };
        set_autonomy(&store, &formation, "suggest").await.unwrap();
        let level = resolve_autonomy(&store, &rule_id, Some(&formation_id)).await;
        assert_eq!(level, AutonomyLevel::Suggest);
        assert_eq!(
            resolve_formation_autonomy(&store, &formation_id).await,
            AutonomyLevel::Suggest
        );

        // Agent row wins over the formation row.
        let agent = AutonomyTarget::Agent { rule_id };
        set_autonomy(&store, &agent, "observe").await.unwrap();
        let level = resolve_autonomy(&store, &rule_id, Some(&formation_id)).await;
        assert_eq!(level, AutonomyLevel::Observe);

        // No formation id: agent row still applies; another rule defaults.
        assert_eq!(
            resolve_autonomy(&store, &rule_id, None).await,
            AutonomyLevel::Observe
        );
        assert_eq!(
            resolve_autonomy(&store, &uuid::Uuid::new_v4(), None).await,
            AutonomyLevel::ActAutonomously
        );

        // Invalid level is rejected at write time.
        assert!(set_autonomy(&store, &agent, "yolo").await.is_err());
    }

    #[test]
    fn test_fuel_pct_half_remaining_returns_50() {
        assert_eq!(fuel_pct(500, 1000), 50);
        assert_eq!(fuel_pct(0, 1000), 0);
        assert_eq!(fuel_pct(1000, 1000), 100);
        assert_eq!(fuel_pct(2000, 1000), 100);
    }

    #[test]
    fn test_fuel_pct_zero_initial_returns_0() {
        assert_eq!(fuel_pct(10, 0), 0);
    }

    #[test]
    fn test_fuel_status_label_thresholds() {
        assert_eq!(fuel_status_label(100), "ok");
        assert_eq!(fuel_status_label(51), "ok");
        assert_eq!(fuel_status_label(50), "warn");
        assert_eq!(fuel_status_label(21), "warn");
        assert_eq!(fuel_status_label(20), "critical");
    }
}

/// Request body for setting an agent's autonomy level.
#[derive(Debug, Clone, serde::Deserialize, utoipa::ToSchema)]
pub struct SetAutonomyRequest {
    /// Level name: `observe`, `suggest`, `approve`, or `autonomous`.
    pub level: String,
}

/// Request body for stepping an agent's autonomy up or down.
#[derive(Debug, Clone, serde::Deserialize, utoipa::ToSchema)]
pub struct StepAutonomyRequest {
    pub direction: AutonomyDirection,
}
