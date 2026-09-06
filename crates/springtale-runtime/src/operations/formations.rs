//! Formation operations — create, deploy, pause, resume, dissolve, list.
//!
//! Uses the cooperation module directly. Formations are the user-facing
//! abstraction over the cooperative agent architecture (COOPERATION.pdf).

use serde::{Deserialize, Serialize};

use specta::Type;
use springtale_core::rule::RuleId;

use crate::operations::formation_synthesis::{
    MemberAutomation, regenerate_formation_rules, store_formation_automation,
};

use crate::error::OperationError;
use crate::operations::config;
use crate::state::RuntimeState;

/// Agent health as a tagged union — mirrors
/// `springtale_cooperation::types::AgentHealth` but owned by the
/// runtime crate so the serialized shape is stable for the
/// frontend/IPC layer (the cooperation type may change its internal
/// representation freely). Matches the TS `AgentHealth` in
/// `tauri/packages/types/src/formation.ts`.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(tag = "type")]
pub enum AgentHealthDetail {
    Operational,
    Degraded { recovery_count: u32 },
    Incapacitated,
    Dead { recoverable: bool },
}

impl From<&springtale_cooperation::types::AgentHealth> for AgentHealthDetail {
    fn from(h: &springtale_cooperation::types::AgentHealth) -> Self {
        use springtale_cooperation::types::AgentHealth;
        match h {
            AgentHealth::Operational => Self::Operational,
            AgentHealth::Degraded { recovery_count } => Self::Degraded {
                recovery_count: *recovery_count,
            },
            AgentHealth::Incapacitated => Self::Incapacitated,
            AgentHealth::Dead { recoverable } => Self::Dead {
                recoverable: *recoverable,
            },
        }
    }
}

impl AgentHealthDetail {
    /// Map to the frontend's CSS-class / data-attribute value. Values
    /// are kept in sync with `tauri/packages/ui/src/colony/*`:
    /// `healthy` = full capability; `degraded` = one quick-fix applied;
    /// `critical` = two quick-fixes (L4D "black & white") OR incapacitated;
    /// `dead` = removed from active work.
    pub fn ui_state(&self) -> &'static str {
        match self {
            Self::Operational => "healthy",
            Self::Degraded { recovery_count } => {
                if *recovery_count >= 2 {
                    "critical"
                } else {
                    "degraded"
                }
            }
            Self::Incapacitated => "critical",
            Self::Dead { .. } => "dead",
        }
    }
}

/// Enriched per-member detail — populated from the bot event loop's
/// in-memory `Formation` data when available.
#[derive(Debug, Clone, Serialize, Type)]
pub struct FormationMemberDetail {
    /// Cooperation-layer agent id as a stable UUID string. Paired with
    /// `connector_name` so UI layers can key by either.
    pub agent_id: String,
    pub connector_name: String,
    pub role: String,
    /// Structured health — replaces the old `Debug`-stringified field.
    /// Serialized as `{"type":"Degraded","recovery_count":2}` etc.,
    /// matching the frontend `AgentHealth` tagged union.
    pub health: AgentHealthDetail,
    pub fuel_remaining: u64,
    /// Fuel the member started with; `fuel_remaining / fuel_initial` is
    /// the live fuel percentage the agent list renders.
    pub fuel_initial: u64,
    pub liveness: String,
    pub attention_load: f32,
    pub active_task: Option<String>,
    pub consecutive_failures: usize,
}

/// Enriched formation detail — `FormationInfo` plus live member details.
#[derive(Debug, Serialize, Type)]
pub struct FormationDetail {
    #[serde(flatten)]
    pub info: FormationInfo,
    pub member_details: Vec<FormationMemberDetail>,
}

/// Badge label for the formation guard toggle.
pub fn guard_status_label(guard_engaged: bool) -> &'static str {
    if guard_engaged { "GUARD" } else { "--" }
}

/// Formation info for listing.
#[derive(Debug, Serialize, Type)]
pub struct FormationInfo {
    pub id: String,
    pub name: String,
    pub intent: String,
    pub status: String,
    pub member_count: usize,
    /// Count of members whose health is Operational or Degraded
    /// (still able to carry work) — drives the "X/Y operational"
    /// row in the BottomPanel aggregate stats.
    pub operational_count: usize,
    /// Connector names of formation members.
    pub members: Vec<String>,
    /// Real momentum tier from runtime — "Cold", "Warming", "Hot", "Fever".
    pub momentum_tier: String,
    /// Human label for momentum tier.
    pub momentum_label: String,
    /// Consecutive successful ticks in the current run. Enables UI
    /// progress bars of the form "5/8 to Hot".
    pub momentum_consecutive_successes: i64,
    /// Lifetime interference total (`MomentumState::interference_total`).
    /// Informational only — a past interference never blocks promotion;
    /// see `momentum.rs`. Wire name kept for the IPC/TS types.
    pub momentum_interference_count: i64,
    /// How many more consecutive successes (at zero interference) are
    /// required to promote to the next tier. `None` at `Fever` (top).
    pub momentum_successes_to_next_tier: Option<i64>,
    /// Capabilities unlocked at current tier.
    pub capabilities: Vec<String>,
    /// Guard badge label derived from `guard_engaged`: "GUARD" when the
    /// guard toggle is engaged, "--" otherwise.
    pub guard_status: String,
    /// True when guard mode is engaged for this formation. Read from the
    /// `guard:{formation_id}` config row — the same key `toggle_formation_guard`
    /// writes (finding 78 / plan 1.12). Gates Dissolve, ChangeIntent,
    /// RemoveMember, and Rally in `commands.rs::is_enabled_for`.
    ///
    /// KNOWN DIVERGENCE: this reads the config row, not the live formation's
    /// `constraints.guard_mode`. The `formation:guard` toggle writes only the
    /// config row; the live `Formation` in the bot tick loop (which
    /// `tick_steps/handle_command.rs::guarded` checks) reads
    /// `constraints.guard_mode`, set once at deploy/spawn time and never
    /// refreshed from the config row afterward. `LiveFormationReader` has no
    /// accessor for a live formation's `constraints.guard_mode` today, so
    /// there is no way to source this field from the live formation without
    /// extending that trait — out of scope for this change. The two can
    /// therefore disagree: toggling guard on a formation whose bot process
    /// already has it live-loaded updates the UI eligibility (this field)
    /// immediately, but the live enforcement in `handle_command.rs` will not
    /// see the change until the formation is redeployed.
    pub guard_engaged: bool,
    /// Rally tokens remaining (Monster Hunter carts, §15).
    pub rally_tokens: i64,
    /// Maximum rally tokens.
    pub rally_max: i64,
}

/// Capabilities unlocked at each momentum tier.
fn tier_capabilities(tier: &str) -> Vec<String> {
    match tier {
        "Cold" => vec!["read env"],
        "Warming" => vec!["read env", "neighbors", "chain"],
        "Hot" => vec!["read env", "neighbors", "chain", "write env", "commit"],
        "Fever" => vec![
            "read env",
            "neighbors",
            "chain",
            "write env",
            "commit",
            "consensus",
            "AI",
            "recruit",
        ],
        _ => vec!["read env"],
    }
    .into_iter()
    .map(String::from)
    .collect()
}

/// Human label for momentum tier.
fn tier_label(tier: &str) -> String {
    match tier {
        "Cold" => "COLD",
        "Warming" => "WARM",
        "Hot" => "HOT",
        "Fever" => "FEVER",
        _ => "COLD",
    }
    .to_owned()
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

/// Deploy a formation — changes status to active and notifies bot event loop.
pub async fn deploy_formation(state: &RuntimeState, id: &str) -> Result<(), OperationError> {
    state.store.update_formation_status(id, "active").await?;
    let fid = springtale_cooperation::types::FormationId::parse(id)
        .map_err(|e| OperationError::Validation(format!("invalid formation id: {e}")))?;
    let _ = state
        .formation_cmd_tx
        .send(springtale_cooperation::command::FormationCommand::Deploy { formation_id: fid })
        .await;
    Ok(())
}

/// Pause a formation.
pub async fn pause_formation(state: &RuntimeState, id: &str) -> Result<(), OperationError> {
    state.store.update_formation_status(id, "paused").await?;
    let fid = springtale_cooperation::types::FormationId::parse(id)
        .map_err(|e| OperationError::Validation(format!("invalid formation id: {e}")))?;
    let _ = state
        .formation_cmd_tx
        .send(springtale_cooperation::command::FormationCommand::Pause { formation_id: fid })
        .await;
    Ok(())
}

/// Resume a paused formation.
pub async fn resume_formation(state: &RuntimeState, id: &str) -> Result<(), OperationError> {
    state.store.update_formation_status(id, "active").await?;
    let fid = springtale_cooperation::types::FormationId::parse(id)
        .map_err(|e| OperationError::Validation(format!("invalid formation id: {e}")))?;
    let _ = state
        .formation_cmd_tx
        .send(springtale_cooperation::command::FormationCommand::Resume { formation_id: fid })
        .await;
    Ok(())
}

/// Dissolve a formation — removes from DB and notifies bot event loop.
pub async fn dissolve_formation(state: &RuntimeState, id: &str) -> Result<(), OperationError> {
    let fid = springtale_cooperation::types::FormationId::parse(id)
        .map_err(|e| OperationError::Validation(format!("invalid formation id: {e}")))?;

    // Tear down the synthesised rules and the automation config so a dissolved
    // formation leaves no orphaned automation behind.
    crate::operations::formation_synthesis::delete_formation_rules(state, fid.0).await?;
    crate::operations::formation_synthesis::clear_formation_automation(state, id).await?;

    state.store.delete_formation(id).await?;
    let _ = state
        .formation_cmd_tx
        .send(
            springtale_cooperation::command::FormationCommand::Dissolve {
                formation_id: fid,
                reason: "user requested".to_owned(),
            },
        )
        .await;
    Ok(())
}

/// Update a formation's intent.
pub async fn update_intent(
    state: &RuntimeState,
    id: &str,
    intent: &str,
) -> Result<(), OperationError> {
    state.store.update_formation_intent(id, intent).await?;
    let fid = springtale_cooperation::types::FormationId::parse(id)
        .map_err(|e| OperationError::Validation(format!("invalid formation id: {e}")))?;
    let parsed = springtale_cooperation::command::parse_intent(intent);
    let _ = state
        .formation_cmd_tx
        .send(
            springtale_cooperation::command::FormationCommand::ChangeIntent {
                formation_id: fid,
                intent: parsed,
            },
        )
        .await;

    // Re-synthesise the formation's persistent rules for the new intent so the
    // outward behaviour actually changes (Reconnoiter → observe, Execute → act),
    // not just the coordination-layer intent. See `formation_synthesis`.
    let name = formation_name(state, id).await;
    regenerate_formation_rules(state, id, &name, intent, false).await?;
    Ok(())
}

/// Look up a formation's display name, falling back to the id if absent.
/// Guard mode is sourced as `false` for rule synthesis: connector actions are
/// `Reversible` (never `Destructive`) so the guard's destructive-downgrade path
/// never changes the synthesised rule set — the live guard toggle continues to
/// gate destructive *coordination* actions in the bot runtime.
async fn formation_name(state: &RuntimeState, id: &str) -> String {
    state
        .store
        .list_formations()
        .await
        .ok()
        .and_then(|fs| fs.into_iter().find(|f| f.id == id).map(|f| f.name))
        .unwrap_or_else(|| id.to_owned())
}

/// Propose an intent change for the formation to vote on (§5.5 source 2).
///
/// Unlike [`update_intent`] (the §3.2 orchestrator/user path, applied
/// immediately), this opens a consensus vote among the formation's
/// operational members. The bot event loop honors it only at Fever
/// (`MomentumState::can_consensus`) — formation self-governance is an
/// earned capability. Joint Intention Theory: the joint goal changes by
/// mutual belief (a vote), not by one member's private belief. Nothing
/// is persisted here; if the vote passes, `resolve_consensus` applies
/// the intent in-memory and the next `update_intent`-style persistence
/// follows the regular formation-state sync.
pub async fn propose_intent_change(
    state: &RuntimeState,
    id: &str,
    intent: &str,
) -> Result<(), OperationError> {
    let fid = springtale_cooperation::types::FormationId::parse(id)
        .map_err(|e| OperationError::Validation(format!("invalid formation id: {e}")))?;
    let parsed = springtale_cooperation::command::parse_intent(intent);
    let _ = state
        .formation_cmd_tx
        .send(
            springtale_cooperation::command::FormationCommand::ProposeIntentChange {
                formation_id: fid,
                intent: parsed,
            },
        )
        .await;
    Ok(())
}

/// Cast a ballot on an open consensus vote (§11).
///
/// `voter` is the agent id casting the ballot; `approve = false` votes
/// for the "deny" option. The vote resolves in the bot's
/// `resolve_consensus` tick step on quorum, override, or deadline.
pub async fn cast_vote(
    state: &RuntimeState,
    formation_id: &str,
    vote_id: &str,
    voter: &str,
    approve: bool,
) -> Result<(), OperationError> {
    let fid = springtale_cooperation::types::FormationId::parse(formation_id)
        .map_err(|e| OperationError::Validation(format!("invalid formation id: {e}")))?;
    let vid = uuid::Uuid::parse_str(vote_id)
        .map_err(|e| OperationError::Validation(format!("invalid vote id: {e}")))?;
    let voter_id = uuid::Uuid::parse_str(voter)
        .map_err(|e| OperationError::Validation(format!("invalid voter id: {e}")))?;
    let _ = state
        .formation_cmd_tx
        .send(
            springtale_cooperation::command::FormationCommand::CastVote {
                formation_id: fid,
                vote_id: vid,
                voter: springtale_cooperation::cadence::AgentId(voter_id),
                approve,
            },
        )
        .await;
    Ok(())
}

/// Manually trigger a self-rally for a formation.
///
/// Sends a Rally command to the bot event loop. The event loop
/// will consume a rally token and redistribute attention.
pub async fn rally_formation(state: &RuntimeState, id: &str) -> Result<(), OperationError> {
    let fid = springtale_cooperation::types::FormationId::parse(id)
        .map_err(|e| OperationError::Validation(format!("invalid formation id: {e}")))?;
    let _ = state
        .formation_cmd_tx
        .send(springtale_cooperation::command::FormationCommand::Rally { formation_id: fid })
        .await;
    Ok(())
}

/// Recruit a new member into a formation — the §7 Fever-tier momentum unlock.
///
/// Unlike [`add_member`], this does NOT persist a member row up front: it is the
/// formation's own earned capability. The bot event loop honors it only when the
/// formation is at Fever and guard mode is off (`MomentumState::can_recruit`).
pub async fn recruit_member(
    state: &RuntimeState,
    formation_id: &str,
    connector_name: &str,
) -> Result<(), OperationError> {
    let fid = springtale_cooperation::types::FormationId::parse(formation_id)
        .map_err(|e| OperationError::Validation(format!("invalid formation id: {e}")))?;
    let _ = state
        .formation_cmd_tx
        .send(springtale_cooperation::command::FormationCommand::Recruit {
            formation_id: fid,
            connector_name: connector_name.to_owned(),
        })
        .await;
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
    let fid = springtale_cooperation::types::FormationId::parse(formation_id)
        .map_err(|e| OperationError::Validation(format!("invalid formation id: {e}")))?;
    let _ = state
        .formation_cmd_tx
        .send(
            springtale_cooperation::command::FormationCommand::AddMember {
                formation_id: fid,
                connector_name: connector_name.to_owned(),
            },
        )
        .await;
    Ok(())
}

/// Remove a member (connector) from a formation.
pub async fn remove_member(
    state: &RuntimeState,
    formation_id: &str,
    connector_name: &str,
) -> Result<(), OperationError> {
    state
        .store
        .delete_formation_member(formation_id, connector_name)
        .await?;
    let fid = springtale_cooperation::types::FormationId::parse(formation_id)
        .map_err(|e| OperationError::Validation(format!("invalid formation id: {e}")))?;
    let _ = state
        .formation_cmd_tx
        .send(
            springtale_cooperation::command::FormationCommand::RemoveMember {
                formation_id: fid,
                connector_name: connector_name.to_owned(),
            },
        )
        .await;
    Ok(())
}

/// Get a single formation by ID with enriched member details.
///
/// When `state.live_formations` is set (daemon mode), this returns
/// live per-member data (role, health, fuel, liveness, attention, task).
/// When `None` (desktop mode), `member_details` is empty.
pub async fn get_formation(
    state: &RuntimeState,
    id: &str,
) -> Result<FormationDetail, OperationError> {
    let formations = list_formations(state).await?;
    let info = formations
        .into_iter()
        .find(|f| f.id == id)
        .ok_or_else(|| OperationError::NotFound(format!("formation {id}")))?;

    let member_details = match &state.live_formations {
        Some(reader) => reader.read_member_details(id).await,
        None => Vec::new(),
    };

    Ok(FormationDetail {
        info,
        member_details,
    })
}

/// Successes required at each tier (§7 promotion thresholds from
/// `momentum.rs`). Kept as a const so frontend progress indicators can
/// show "N more ticks to next tier" without a round-trip to cooperation.
const SUCCESSES_REQUIRED_AT: &[(&str, i64)] = &[
    ("Cold", 3),
    ("Warming", 8),
    ("Hot", 15),
    // Fever has no next tier; absent entry → `None` in
    // `momentum_successes_to_next_tier`.
];

fn successes_to_next_tier(tier: &str, consecutive: i64) -> Option<i64> {
    let target = SUCCESSES_REQUIRED_AT
        .iter()
        .find(|(t, _)| *t == tier)
        .map(|(_, n)| *n)?;
    Some((target - consecutive).max(0))
}

/// List all formations with member counts.
pub async fn list_formations(state: &RuntimeState) -> Result<Vec<FormationInfo>, OperationError> {
    let formations = state.store.list_formations().await?;
    let mut infos = Vec::new();

    for f in formations {
        let members = state.store.list_formation_members(&f.id).await?;
        let member_names: Vec<String> = members.iter().map(|m| m.connector_name.clone()).collect();

        // Read persisted momentum from dedicated table (written by bot event loop).
        let momentum_row = state
            .store
            .get_formation_momentum(&f.id)
            .await
            .ok()
            .flatten();
        let (momentum_tier, momentum_consecutive_successes, momentum_interference_count) =
            match momentum_row.as_ref() {
                Some(r) => (
                    r.tier.clone(),
                    r.consecutive_successes,
                    r.interference_count,
                ),
                // No momentum row → brand-new formation still at Cold
                // with zero counters. `list_formations` never writes, so
                // this default is purely a display fallback.
                None => ("Cold".to_owned(), 0, 0),
            };

        let momentum_successes_to_next_tier =
            successes_to_next_tier(&momentum_tier, momentum_consecutive_successes);

        // Read rally state from dedicated table
        let rally_row = state.store.get_formation_rally(&f.id).await.ok().flatten();
        // No rally row → the formation has never been given a rally
        // budget; report zero so the UI hides the pips instead of
        // inventing a full set.
        let rally_tokens = rally_row.as_ref().map(|r| r.tokens_remaining).unwrap_or(0);
        let rally_max = rally_row.as_ref().map(|r| r.max_tokens).unwrap_or(0);

        let momentum_label = tier_label(&momentum_tier);
        let capabilities = tier_capabilities(&momentum_tier);
        // See `guard_engaged` doc comment for the live-vs-config divergence.
        let guard_engaged = !config::get_config(&*state.store, &format!("guard:{}", f.id))
            .await
            .unwrap_or(serde_json::Value::Null)
            .is_null();
        let guard_status = guard_status_label(guard_engaged).to_owned();

        // Operational count: prefer the live reader (accurate — reads
        // current AgentHealth from in-memory Formation). Fall back to
        // member_count when no reader is wired (desktop app before
        // daemon connection). The fallback over-reports; the UI can
        // still display something sensible in that case.
        let operational_count = match &state.live_formations {
            Some(reader) => {
                let details = reader.read_member_details(&f.id).await;
                if details.is_empty() {
                    member_names.len()
                } else {
                    details
                        .iter()
                        .filter(|d| {
                            matches!(
                                d.health,
                                AgentHealthDetail::Operational | AgentHealthDetail::Degraded { .. }
                            )
                        })
                        .count()
                }
            }
            None => member_names.len(),
        };

        infos.push(FormationInfo {
            id: f.id,
            name: f.name,
            intent: f.intent,
            status: f.status,
            member_count: member_names.len(),
            operational_count,
            members: member_names,
            momentum_tier,
            momentum_label,
            momentum_consecutive_successes,
            momentum_interference_count,
            momentum_successes_to_next_tier,
            capabilities,
            guard_status,
            guard_engaged,
            rally_tokens,
            rally_max,
        });
    }

    Ok(infos)
}

// ── Team deploy (atomic OOBE operation) ─────────────────────────────────────

/// Single agent slot in a team deploy request.
#[derive(Debug, Deserialize, Type)]
#[serde(deny_unknown_fields)]
pub struct TeamAgentSlot {
    pub connector_name: String,
    pub trigger_name: String,
    pub action_connector: String,
    pub action_name: String,
}

/// Request to deploy a complete team — creates rules + formation atomically.
#[derive(Debug, Deserialize, Type)]
#[serde(deny_unknown_fields)]
pub struct TeamDeployRequest {
    pub name: String,
    pub intent: String,
    pub agents: Vec<TeamAgentSlot>,
    pub guard_mode: bool,
}

/// Result of a team deploy.
#[derive(Debug, Serialize, Type)]
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

    // 1. Per-member automation config — the durable source of truth the
    //    formation's rules are derived from. Persisted so intent cycling can
    //    re-synthesise rules non-lossily (see `formation_synthesis`).
    let automations: Vec<MemberAutomation> = team
        .agents
        .iter()
        .filter(|a| !a.connector_name.is_empty() && !a.trigger_name.is_empty())
        .map(|a| MemberAutomation {
            connector_name: a.connector_name.clone(),
            trigger_name: a.trigger_name.clone(),
            action_connector: a.action_connector.clone(),
            action_name: a.action_name.clone(),
            params: serde_json::Map::new(),
        })
        .collect();

    // 2. Derive unique connector names (formation members).
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

    // 3. Create the formation first — its id owns the synthesised rules.
    let formation_id = create_formation(
        state,
        team.name.clone(),
        team.intent.clone(),
        connector_names,
    )
    .await?;

    // 4. Persist automation, then synthesise formation-scoped rules for the
    //    initial intent. On failure, roll back the formation we just created.
    store_formation_automation(state, &formation_id, &automations).await?;
    let created_rule_ids: Vec<RuleId> = match regenerate_formation_rules(
        state,
        &formation_id,
        &team.name,
        &team.intent,
        team.guard_mode,
    )
    .await
    {
        Ok(ids) => ids,
        Err(e) => {
            let _ = dissolve_formation(state, &formation_id).await;
            return Err(e);
        }
    };

    // 5. Deploy and mark onboarding complete.
    deploy_formation(state, &formation_id).await?;
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
#[derive(Debug, Serialize, Type)]
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

    let fid = springtale_cooperation::types::FormationId::parse(formation_id)
        .map_err(|e| OperationError::Validation(format!("invalid formation id: {e}")))?;
    let parsed = springtale_cooperation::command::parse_intent(next);
    let _ = state
        .formation_cmd_tx
        .send(
            springtale_cooperation::command::FormationCommand::ChangeIntent {
                formation_id: fid,
                intent: parsed,
            },
        )
        .await;

    // Re-synthesise persistent rules for the new intent (non-lossy via the
    // stored automation config).
    regenerate_formation_rules(state, formation_id, &formation.name, next, false).await?;

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
    let target = super::agent::AutonomyTarget::Formation {
        id: formation_id.to_owned(),
    };
    let current = super::agent::get_autonomy(&*state.store, &target).await?;

    let levels = [
        "observe",
        "suggest",
        "act-with-approval",
        "act-autonomously",
    ];
    let idx = levels
        .iter()
        .position(|l| *l == current.as_str())
        .unwrap_or(0);
    let next = levels[(idx + 1) % levels.len()];

    super::agent::set_autonomy(&*state.store, &target, next).await?;

    Ok(next.to_owned())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn successes_to_next_tier_cold_to_warming() {
        assert_eq!(successes_to_next_tier("Cold", 0), Some(3));
        assert_eq!(successes_to_next_tier("Cold", 2), Some(1));
        // Saturating — at threshold or past, remaining is 0.
        assert_eq!(successes_to_next_tier("Cold", 3), Some(0));
        assert_eq!(successes_to_next_tier("Cold", 5), Some(0));
    }

    #[test]
    fn successes_to_next_tier_warming_and_hot() {
        assert_eq!(successes_to_next_tier("Warming", 4), Some(4));
        assert_eq!(successes_to_next_tier("Hot", 10), Some(5));
    }

    #[test]
    fn successes_to_next_tier_fever_is_none() {
        // Fever is the top tier — no promotion target, so no number
        // makes sense. UI renders this as "MAX" or hides the row.
        assert_eq!(successes_to_next_tier("Fever", 99), None);
    }

    #[test]
    fn ui_state_maps_healthy_degraded_critical_dead() {
        assert_eq!(AgentHealthDetail::Operational.ui_state(), "healthy");
        assert_eq!(
            AgentHealthDetail::Degraded { recovery_count: 1 }.ui_state(),
            "degraded"
        );
        // L4D "black & white" — second quick-fix.
        assert_eq!(
            AgentHealthDetail::Degraded { recovery_count: 2 }.ui_state(),
            "critical"
        );
        assert_eq!(AgentHealthDetail::Incapacitated.ui_state(), "critical");
        assert_eq!(
            AgentHealthDetail::Dead { recoverable: true }.ui_state(),
            "dead"
        );
        assert_eq!(
            AgentHealthDetail::Dead { recoverable: false }.ui_state(),
            "dead"
        );
    }

    #[test]
    fn from_cooperation_agent_health_roundtrips() {
        use springtale_cooperation::types::AgentHealth;

        let cases = [
            (AgentHealth::Operational, AgentHealthDetail::Operational),
            (
                AgentHealth::Degraded { recovery_count: 2 },
                AgentHealthDetail::Degraded { recovery_count: 2 },
            ),
            (AgentHealth::Incapacitated, AgentHealthDetail::Incapacitated),
            (
                AgentHealth::Dead { recoverable: false },
                AgentHealthDetail::Dead { recoverable: false },
            ),
        ];
        for (coop, expected) in cases {
            let converted = AgentHealthDetail::from(&coop);
            // Compare via serde JSON — both implementations serialize
            // to the same tagged shape, and `PartialEq` isn't derived
            // on either side.
            let got = serde_json::to_value(&converted).unwrap();
            let want = serde_json::to_value(&expected).unwrap();
            assert_eq!(got, want);
        }
    }

    #[test]
    fn agent_health_detail_serialization_matches_ts_shape() {
        // The frontend TS type is
        // `{ type: "Degraded"; recovery_count: number }` — serde's
        // `#[serde(tag = "type")]` is what makes that compatible.
        // Lock the shape in a test so any accidental change to the
        // enum attributes is caught immediately.
        let health = AgentHealthDetail::Degraded { recovery_count: 2 };
        let json = serde_json::to_value(&health).unwrap();
        assert_eq!(json["type"], "Degraded");
        assert_eq!(json["recovery_count"], 2);

        let op = serde_json::to_value(AgentHealthDetail::Operational).unwrap();
        assert_eq!(op, serde_json::json!({"type": "Operational"}));

        let dead = serde_json::to_value(AgentHealthDetail::Dead { recoverable: false }).unwrap();
        assert_eq!(
            dead,
            serde_json::json!({"type": "Dead", "recoverable": false})
        );
    }

    #[test]
    fn formation_info_serialization_carries_new_momentum_fields() {
        // Locks the wire contract so a rename or reorder breaks the
        // test, not the frontend.
        let info = FormationInfo {
            id: "id-1".into(),
            name: "Monitor".into(),
            intent: "Reconnoiter".into(),
            status: "active".into(),
            member_count: 2,
            operational_count: 2,
            members: vec!["slack".into(), "github".into()],
            momentum_tier: "Warming".into(),
            momentum_label: "WARM".into(),
            momentum_consecutive_successes: 5,
            momentum_interference_count: 0,
            momentum_successes_to_next_tier: Some(3),
            capabilities: vec!["read env".into(), "chain".into()],
            guard_status: "GUARD".into(),
            guard_engaged: true,
            rally_tokens: 3,
            rally_max: 3,
        };
        let json = serde_json::to_value(&info).unwrap();
        // Every field the frontend consumes must be present.
        for key in [
            "id",
            "name",
            "intent",
            "status",
            "member_count",
            "operational_count",
            "members",
            "momentum_tier",
            "momentum_label",
            "momentum_consecutive_successes",
            "momentum_interference_count",
            "momentum_successes_to_next_tier",
            "capabilities",
            "guard_status",
            "rally_tokens",
            "rally_max",
        ] {
            assert!(
                json.get(key).is_some(),
                "FormationInfo should serialize `{key}`"
            );
        }
        assert_eq!(json["momentum_consecutive_successes"], 5);
        assert_eq!(json["momentum_successes_to_next_tier"], 3);
    }

    #[test]
    fn formation_info_successes_to_next_tier_null_at_fever() {
        // At Fever, `momentum_successes_to_next_tier` is `None`; serde
        // serializes to JSON `null`. The frontend interprets this as
        // "no further tier to promote to" and hides the progress bar.
        let info = FormationInfo {
            id: "id-2".into(),
            name: "Surge".into(),
            intent: "Surge".into(),
            status: "active".into(),
            member_count: 3,
            operational_count: 3,
            members: vec![],
            momentum_tier: "Fever".into(),
            momentum_label: "FEVER".into(),
            momentum_consecutive_successes: 42,
            momentum_interference_count: 0,
            momentum_successes_to_next_tier: None,
            capabilities: vec![],
            guard_status: "OK".into(),
            guard_engaged: false,
            rally_tokens: 3,
            rally_max: 3,
        };
        let json = serde_json::to_value(&info).unwrap();
        assert!(json["momentum_successes_to_next_tier"].is_null());
    }

    #[test]
    fn test_guard_status_label_engaged_returns_guard() {
        assert_eq!(guard_status_label(true), "GUARD");
        assert_eq!(guard_status_label(false), "--");
    }
}
