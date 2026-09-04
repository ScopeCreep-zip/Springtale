//! Intent → formation-scoped rule synthesis.
//!
//! Closes the founding-architecture gap tracked in `docs/arch/AUDIT-NOTES.md §4`:
//! a formation's `IntentPattern` now compiles into persistent, formation-scoped
//! `Rule` rows, deterministically and with no AI involvement (the "AI is
//! optional augmentation" invariant — see `springtale-bot`
//! `orchestrator::orchestrate::decompose_intent_deterministic` for the live
//! subtask counterpart).
//!
//! ## Why a separate automation config
//!
//! The per-member automation (`connector`, `trigger`, `action`) is persisted in
//! the config store under `formation:{id}:automation`, *separately* from the
//! live rules it generates. Rules are then always fully derived from
//! `(automation × intent)`. This makes intent cycling **non-lossy**: a
//! `Reconnoiter` rule downgrades its action to a read-only observation, but the
//! canonical action survives in the automation config, so cycling back to
//! `Execute` restores it exactly.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use springtale_connector::registry::store::ConnectorRegistry;
use springtale_core::rule::{Action, Rule, RuleId, RuleOwner, RuleStatus, RuleVersion, Trigger};
use springtale_sentinel::impact::{ActionHints, ActionImpact, classify_impact};

use crate::error::OperationError;
use crate::operations::{config, rules};
use crate::state::RuntimeState;

/// One member's automation — the durable source of truth a formation's rules
/// are derived from. Holds the canonical (mutating) action independent of the
/// current intent so regeneration is non-lossy.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemberAutomation {
    pub connector_name: String,
    pub trigger_name: String,
    #[serde(default)]
    pub action_connector: String,
    #[serde(default)]
    pub action_name: String,
    #[serde(default)]
    pub params: serde_json::Map<String, serde_json::Value>,
}

fn automation_key(formation_id: &str) -> String {
    format!("formation:{formation_id}:automation")
}

/// Manifest hints for the actions a formation's automations name, keyed
/// by `(action_connector, action_name)`. Built from the connector
/// registry by [`collect_action_hints`] and consulted by
/// [`synthesize_formation_rules`] so guard mode classifies a
/// `RunConnector` the same way the sentinel does at dispatch.
pub type ActionHintIndex = HashMap<(String, String), ActionHints>;

/// Look up the manifest's advisory hints for every action the given
/// automations reference. Actions whose connector is not installed, or
/// which the manifest does not declare, are simply absent — and
/// [`classify_impact`] treats an absent hint as destructive.
pub fn collect_action_hints(
    registry: &ConnectorRegistry,
    automations: &[MemberAutomation],
) -> ActionHintIndex {
    let mut out = ActionHintIndex::new();
    for auto in automations {
        if auto.action_connector.is_empty() || auto.action_name.is_empty() {
            continue;
        }
        let key = (auto.action_connector.clone(), auto.action_name.clone());
        if out.contains_key(&key) {
            continue;
        }
        let hints = registry.get(&auto.action_connector).and_then(|entry| {
            entry
                .host
                .actions()
                .iter()
                .find(|decl| decl.name == auto.action_name)
                .map(|decl| ActionHints {
                    read_only: decl.read_only,
                    destructive: decl.destructive,
                })
        });
        if let Some(hints) = hints {
            out.insert(key, hints);
        }
    }
    out
}

/// Read-only observation action used under Reconnoiter / Stabilize, or when an
/// Execute action would be destructive while guard mode is engaged.
fn observation_action(formation_name: &str, auto: &MemberAutomation) -> Action {
    Action::Notify {
        title: format!("{formation_name}: {} event", auto.trigger_name),
        body: format!(
            "Observed `{}` on `{}` (read-only intent).",
            auto.trigger_name, auto.connector_name
        ),
    }
}

/// Compile a formation's automation + intent into formation-scoped rules.
///
/// Intent semantics:
/// - **Reconnoiter** (monitor, read-only) / **Stabilize** (maintain) → each
///   member trigger fires a read-only `Notify` observation; the mutating action
///   is never invoked.
/// - **Execute** / **Surge** (take action) → each member trigger fires its
///   configured `RunConnector` action. Under guard mode, an action sentinel
///   classifies as `Destructive` — consulting the manifest hints in `hints`,
///   with an absent hint counting as destructive — is downgraded to an
///   observation instead.
/// - **Dissolve** / unknown → no rules.
///
/// Every rule is owned by `RuleOwner::Formation { formation_id }` so it only
/// fires in this formation's context (enforced at `RuleEngine::evaluate`).
pub fn synthesize_formation_rules(
    formation_id: uuid::Uuid,
    formation_name: &str,
    intent: &str,
    guard_mode: bool,
    automations: &[MemberAutomation],
    hints: &ActionHintIndex,
) -> Vec<Rule> {
    let active = matches!(intent, "Reconnoiter" | "Execute" | "Stabilize" | "Surge");
    if !active {
        return Vec::new(); // Dissolve / unknown → wind down, no rules.
    }

    let mut out = Vec::new();
    for auto in automations {
        if auto.connector_name.is_empty() || auto.trigger_name.is_empty() {
            continue;
        }

        let trigger = Trigger::ConnectorEvent {
            connector: auto.connector_name.clone(),
            event: auto.trigger_name.clone(),
        };

        let action = match intent {
            "Execute" | "Surge" if !auto.action_name.is_empty() => {
                let run = Action::RunConnector {
                    connector: auto.action_connector.clone(),
                    action: auto.action_name.clone(),
                    params: auto.params.clone(),
                };
                // Guard engaged → refuse destructive actions (mirrors the
                // formation guard semantics documented in guide/formations.md).
                let hint = hints
                    .get(&(auto.action_connector.clone(), auto.action_name.clone()))
                    .copied();
                if guard_mode && classify_impact(&run, hint) == ActionImpact::Destructive {
                    observation_action(formation_name, auto)
                } else {
                    run
                }
            }
            // Reconnoiter, Stabilize, or Execute/Surge with no action configured.
            _ => observation_action(formation_name, auto),
        };

        out.push(Rule {
            id: RuleId::new(),
            name: format!("{formation_name} — {} ({intent})", auto.trigger_name),
            description: format!("Auto-derived from formation intent `{intent}`."),
            status: RuleStatus::Enabled,
            version: RuleVersion(1),
            trigger,
            conditions: Vec::new(),
            actions: vec![action],
            owner: RuleOwner::Formation { formation_id },
        });
    }
    out
}

/// Persist a formation's per-member automation config (source of truth for
/// rule regeneration). Call once at deploy time.
pub async fn store_formation_automation(
    state: &RuntimeState,
    formation_id: &str,
    automations: &[MemberAutomation],
) -> Result<(), OperationError> {
    let value = serde_json::to_value(automations)
        .map_err(|e| OperationError::Validation(format!("serialize automation: {e}")))?;
    config::set_config(&*state.store, &automation_key(formation_id), value).await
}

/// Load a formation's automation config. Empty when none was stored.
pub async fn load_formation_automation(
    state: &RuntimeState,
    formation_id: &str,
) -> Result<Vec<MemberAutomation>, OperationError> {
    let raw = config::get_config(&*state.store, &automation_key(formation_id)).await?;
    if raw.is_null() {
        return Ok(Vec::new());
    }
    serde_json::from_value(raw)
        .map_err(|e| OperationError::Validation(format!("parse automation config: {e}")))
}

/// Delete the config store entry for a formation's automation (on dissolve).
pub async fn clear_formation_automation(
    state: &RuntimeState,
    formation_id: &str,
) -> Result<(), OperationError> {
    // Setting to an empty array is equivalent to "no automation" for our
    // load path and keeps a single code path through the config store.
    store_formation_automation(state, formation_id, &[]).await
}

/// Delete every rule currently owned by this formation.
pub async fn delete_formation_rules(
    state: &RuntimeState,
    formation_id: uuid::Uuid,
) -> Result<(), OperationError> {
    // The store has no owner index, so filter the engine's authoritative list
    // in-app via `RuleOwner::matches`.
    let ids: Vec<RuleId> = {
        let engine = state.engine.read().await;
        engine
            .list_rules()
            .iter()
            .filter(|r| r.owner.matches(None, Some(formation_id)))
            // `Global` rules also match a formation context — exclude them; we
            // only ever delete rules this formation actually owns.
            .filter(|r| matches!(r.owner, RuleOwner::Formation { .. }))
            .map(|r| r.id)
            .collect()
    };
    for id in &ids {
        rules::delete_rule(state, id).await?;
    }
    Ok(())
}

/// Replace a formation's rules to match a (possibly new) intent.
///
/// Deletes the formation's existing rules, reloads its automation config, and
/// recreates the rule set for `intent`. Idempotent and non-lossy.
pub async fn regenerate_formation_rules(
    state: &RuntimeState,
    formation_id: &str,
    formation_name: &str,
    intent: &str,
    guard_mode: bool,
) -> Result<Vec<RuleId>, OperationError> {
    let fid = uuid::Uuid::parse_str(formation_id)
        .map_err(|e| OperationError::Validation(format!("invalid formation id: {e}")))?;

    delete_formation_rules(state, fid).await?;

    let automations = load_formation_automation(state, formation_id).await?;
    let hints = {
        let registry = state.registry.read().await;
        collect_action_hints(&registry, &automations)
    };
    let new_rules = synthesize_formation_rules(
        fid,
        formation_name,
        intent,
        guard_mode,
        &automations,
        &hints,
    );

    let mut created = Vec::with_capacity(new_rules.len());
    for rule in new_rules {
        created.push(rules::create_rule(state, rule).await?);
    }
    Ok(created)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn auto(conn: &str, trig: &str, action: &str) -> MemberAutomation {
        MemberAutomation {
            connector_name: conn.to_owned(),
            trigger_name: trig.to_owned(),
            action_connector: conn.to_owned(),
            action_name: action.to_owned(),
            params: serde_json::Map::new(),
        }
    }

    #[test]
    fn reconnoiter_is_read_only() {
        let fid = uuid::Uuid::new_v4();
        let rules = synthesize_formation_rules(
            fid,
            "Squad",
            "Reconnoiter",
            false,
            &[auto(
                "connector-telegram",
                "message_received",
                "send_message",
            )],
            &ActionHintIndex::new(),
        );
        assert_eq!(rules.len(), 1);
        assert!(matches!(rules[0].actions[0], Action::Notify { .. }));
        assert!(
            matches!(rules[0].owner, RuleOwner::Formation { formation_id } if formation_id == fid)
        );
    }

    #[test]
    fn execute_runs_the_configured_action() {
        let fid = uuid::Uuid::new_v4();
        let rules = synthesize_formation_rules(
            fid,
            "Squad",
            "Execute",
            false,
            &[auto(
                "connector-telegram",
                "message_received",
                "send_message",
            )],
            &ActionHintIndex::new(),
        );
        assert_eq!(rules.len(), 1);
        match &rules[0].actions[0] {
            Action::RunConnector { action, .. } => assert_eq!(action, "send_message"),
            other => panic!("expected RunConnector, got {other:?}"),
        }
    }

    #[test]
    fn dissolve_emits_no_rules() {
        let fid = uuid::Uuid::new_v4();
        let rules = synthesize_formation_rules(
            fid,
            "Squad",
            "Dissolve",
            false,
            &[auto(
                "connector-telegram",
                "message_received",
                "send_message",
            )],
            &ActionHintIndex::new(),
        );
        assert!(rules.is_empty());
    }

    #[test]
    fn guard_downgrades_hintless_execute_to_observation() {
        let fid = uuid::Uuid::new_v4();
        // No manifest hint for the action → `classify_impact` treats the
        // `RunConnector` as Destructive (MCP `destructiveHint` default),
        // so guard mode downgrades it to a read-only observation.
        let rules = synthesize_formation_rules(
            fid,
            "Squad",
            "Execute",
            true,
            &[auto("connector-fs", "file_changed", "write_file")],
            &ActionHintIndex::new(),
        );
        assert!(matches!(rules[0].actions[0], Action::Notify { .. }));
    }

    #[test]
    fn guard_keeps_execute_when_manifest_says_not_destructive() {
        let fid = uuid::Uuid::new_v4();
        let mut hints = ActionHintIndex::new();
        hints.insert(
            ("connector-fs".to_owned(), "write_file".to_owned()),
            ActionHints {
                read_only: false,
                destructive: Some(false),
            },
        );
        let rules = synthesize_formation_rules(
            fid,
            "Squad",
            "Execute",
            true,
            &[auto("connector-fs", "file_changed", "write_file")],
            &hints,
        );
        assert!(matches!(rules[0].actions[0], Action::RunConnector { .. }));
    }
}
