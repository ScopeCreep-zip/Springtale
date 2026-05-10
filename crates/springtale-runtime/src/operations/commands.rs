//! Backend-supplied UI command and member descriptors — keeps the frontend
//! thin (no eligibility logic) and keeps the canonical command set in one
//! place (Rust). Mirrors the formation 3×3 grid in
//! `docs/guide/colony-canvas.md §3`.
//!
//! Surfaces:
//! - `apps/springtaled` — `GET /formations/{id}/commands`,
//!   `GET /formations/{id}/members/eligible`
//! - `tauri/apps/desktop/src-tauri` — `#[tauri::command] formation_commands`,
//!   `formation_eligible_members`
//!
//! Both surfaces call the same functions here. Rendering is whatever the
//! frontend wants; gating is decided in Rust.

use serde::Serialize;

use crate::error::OperationError;
use crate::operations::formations::{FormationDetail, get_formation};
use crate::state::RuntimeState;

/// Declarative command descriptor sent to the UI. The frontend renders the
/// list as-is and dispatches by `id`. `enabled = false` items render greyed
/// with `disabled_reason` as a tooltip.
#[derive(Debug, Clone, Serialize)]
pub struct CommandDecl {
    /// Stable command id, e.g. `"formation:deploy"`. The frontend matches on
    /// this when dispatching to the right handler.
    pub id: String,
    /// Human label shown on the button, e.g. `"DEPLOY"`.
    pub label: String,
    /// Pixel icon character, e.g. `">"`.
    pub icon: String,
    /// Canonical hotkey, decided here so it stays the same on every surface.
    pub hotkey: String,
    /// Whether the command is currently usable.
    pub enabled: bool,
    /// One-line reason shown when `enabled = false` (tooltip / aria-label).
    pub disabled_reason: Option<String>,
}

/// Eligible-removal target for the `RM MBR` overlay. Backend decides which
/// members are removable so the frontend never has to enforce invariants
/// (e.g. "you can't remove the last member; use DISSOLVE instead").
#[derive(Debug, Clone, Serialize)]
pub struct MemberRef {
    /// Cooperation-layer agent id (UUID string).
    pub agent_id: String,
    /// Connector name — what `remove_formation_member` keys on.
    pub connector_name: String,
    /// Role name from `DynamicRoleTrait::name()` (e.g. "scout", "guard").
    pub role: String,
    pub can_remove: bool,
    /// One-line reason when `can_remove = false`.
    pub block_reason: Option<String>,
}

/// Canonical 3×3 formation command grid (`docs/guide/colony-canvas.md §3`).
///
/// Order matters: render row-major. AI / AUTONOMY / FUEL etc. belong to
/// agent context per §3 ("Agent selected: Autonomy cycle…"), not formation,
/// and so are NOT in this list.
const FORMATION_COMMANDS_CANONICAL: &[(&str, &str, &str, &str)] = &[
    // (id, label, icon, hotkey)
    ("formation:deploy", "DEPLOY", ">", "Y"),
    ("formation:pause", "PAUSE", "||", "P"),
    ("formation:resume", "RESUME", ">>", "M"),
    ("formation:rally", "RALLY", "!", "R"),
    ("formation:intent", "INTENT", "I", "I"),
    ("formation:guard", "GUARD", "#", "G"),
    ("formation:add_member", "ADD MBR", "+", "A"),
    ("formation:remove_member", "RM MBR", "-", "X"),
    ("formation:dissolve", "REMOVE", "x", "D"),
];

/// Build the formation command list with status-aware enable/disable.
pub async fn formation_available_commands(
    state: &RuntimeState,
    id: &str,
) -> Result<Vec<CommandDecl>, OperationError> {
    let detail = get_formation(state, id).await?;
    let mut out = Vec::with_capacity(FORMATION_COMMANDS_CANONICAL.len());
    for (cid, label, icon, hotkey) in FORMATION_COMMANDS_CANONICAL {
        let (enabled, reason) = is_enabled_for(cid, &detail);
        out.push(CommandDecl {
            id: (*cid).to_owned(),
            label: (*label).to_owned(),
            icon: (*icon).to_owned(),
            hotkey: (*hotkey).to_owned(),
            enabled,
            disabled_reason: reason,
        });
    }
    Ok(out)
}

/// Decide whether a single command is enabled given a formation's state.
/// Rule of thumb: lifecycle commands gate by `status`; capability commands
/// (RALLY, ADD MBR, RM MBR) gate by their resource invariant.
fn is_enabled_for(cid: &str, d: &FormationDetail) -> (bool, Option<String>) {
    let status = d.info.status.as_str();
    let member_count = d.info.member_count;
    let rally_remaining = d.info.rally_tokens;
    match cid {
        "formation:deploy" => match status {
            "draft" => (true, None),
            other => (false, Some(format!("formation already {other}"))),
        },
        "formation:pause" => match status {
            "active" => (true, None),
            other => (false, Some(format!("can only pause an active formation (currently {other})"))),
        },
        "formation:resume" => match status {
            "paused" => (true, None),
            other => (false, Some(format!("can only resume a paused formation (currently {other})"))),
        },
        "formation:rally" => {
            if status != "active" {
                (false, Some("rally only applies to active formations".to_owned()))
            } else if rally_remaining <= 0 {
                (false, Some("no rally tokens remaining".to_owned()))
            } else {
                (true, None)
            }
        }
        "formation:add_member" => (true, None),
        "formation:remove_member" => {
            if member_count > 1 {
                (true, None)
            } else {
                (
                    false,
                    Some("cannot remove the last member — use DISSOLVE instead".to_owned()),
                )
            }
        }
        // INTENT, GUARD, DISSOLVE always enabled when a formation exists.
        _ => (true, None),
    }
}

/// Build the eligible-removal list for the RM MBR overlay. `can_remove =
/// false` rows are displayed but not clickable — the frontend just renders.
pub async fn formation_eligible_members(
    state: &RuntimeState,
    id: &str,
) -> Result<Vec<MemberRef>, OperationError> {
    let detail = get_formation(state, id).await?;
    let total = detail.member_details.len();
    let last_member = total <= 1;
    let block_reason = last_member
        .then(|| "use DISSOLVE to remove the last member".to_owned());
    Ok(detail
        .member_details
        .into_iter()
        .map(|m| MemberRef {
            agent_id: m.agent_id,
            connector_name: m.connector_name,
            role: m.role,
            can_remove: !last_member,
            block_reason: block_reason.clone(),
        })
        .collect())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::operations::formations::{AgentHealthDetail, FormationInfo, FormationMemberDetail};

    fn make_detail(status: &str, members: usize, rally: i64) -> FormationDetail {
        FormationDetail {
            info: FormationInfo {
                id: "test-formation".into(),
                name: "Test".into(),
                intent: "Reconnoiter".into(),
                status: status.into(),
                member_count: members,
                operational_count: members,
                members: (0..members).map(|i| format!("connector-{i}")).collect(),
                momentum_tier: "Cold".into(),
                momentum_label: "COLD".into(),
                momentum_consecutive_successes: 0,
                momentum_interference_count: 0,
                momentum_successes_to_next_tier: Some(3),
                capabilities: vec![],
                guard_status: "--".into(),
                rally_tokens: rally,
                rally_max: 3,
            },
            member_details: (0..members)
                .map(|i| FormationMemberDetail {
                    agent_id: format!("agent-{i}"),
                    connector_name: format!("connector-{i}"),
                    role: "scout".into(),
                    health: AgentHealthDetail::Operational,
                    fuel_remaining: 100,
                    liveness: "Alive".into(),
                    attention_load: 0.0,
                    active_task: None,
                    consecutive_failures: 0,
                })
                .collect(),
        }
    }

    #[test]
    fn deploy_only_enabled_in_draft() {
        let draft = make_detail("draft", 2, 3);
        let active = make_detail("active", 2, 3);
        assert!(is_enabled_for("formation:deploy", &draft).0);
        assert!(!is_enabled_for("formation:deploy", &active).0);
    }

    #[test]
    fn pause_only_enabled_when_active() {
        let active = make_detail("active", 2, 3);
        let paused = make_detail("paused", 2, 3);
        assert!(is_enabled_for("formation:pause", &active).0);
        assert!(!is_enabled_for("formation:pause", &paused).0);
    }

    #[test]
    fn resume_only_enabled_when_paused() {
        let paused = make_detail("paused", 2, 3);
        let active = make_detail("active", 2, 3);
        assert!(is_enabled_for("formation:resume", &paused).0);
        assert!(!is_enabled_for("formation:resume", &active).0);
    }

    #[test]
    fn rally_disabled_when_no_tokens() {
        let no_tokens = make_detail("active", 2, 0);
        let some_tokens = make_detail("active", 2, 1);
        assert!(!is_enabled_for("formation:rally", &no_tokens).0);
        assert!(is_enabled_for("formation:rally", &some_tokens).0);
    }

    #[test]
    fn rally_disabled_when_inactive() {
        let draft = make_detail("draft", 2, 3);
        assert!(!is_enabled_for("formation:rally", &draft).0);
    }

    #[test]
    fn remove_member_blocked_for_last_member() {
        let one = make_detail("active", 1, 3);
        let two = make_detail("active", 2, 3);
        assert!(!is_enabled_for("formation:remove_member", &one).0);
        assert!(is_enabled_for("formation:remove_member", &two).0);
    }
}
