//! ## `specta::Type` policy in this module
//!
//! Leaf types that show up in flat IPC projections (`RuleSummary { id:
//! RuleId, … }`, `RuleDetail`, etc.) keep `Type`:
//!   - [`RuleId`], [`RuleVersion`], [`RuleStatus`]
//!
//! Recursive / composite rule types do NOT derive `Type`:
//!   - [`Rule`] (contains `Vec<Action>` + `Vec<Condition>`)
//!   - [`crate::rule::action::Action`] (`Chain { steps: Vec<Action> }`)
//!   - [`crate::rule::condition::Condition`] (`And`/`Or`/`Not`)
//!   - [`crate::rule::trigger::Trigger`] (nested in `Rule`)
//!
//! Why: `specta` v2.0.0-rc.25 stack-overflows on self-referential
//! enums during `Builder.export()` type-graph traversal. More
//! importantly, no Tauri command takes `Rule` as a typed parameter —
//! `create_rule` / `update_rule` accept `serde_json::Value` and
//! deserialize internally. The rule builder UI reads
//! `get_rule_schema()`'s JsonSchema (`schemars`) to render its
//! form, not specta bindings. This matches Spacedrive's "flat
//! projections only over IPC" pattern; do not re-add `Type` here.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use specta::Type;
use uuid::Uuid;

use super::action::Action;
use super::condition::Condition;
use super::trigger::Trigger;

/// Unique identifier for a rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, Type)]
pub struct RuleId(pub Uuid);

impl RuleId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for RuleId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for RuleId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Monotonically increasing version for rule updates.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, Type,
)]
pub struct RuleVersion(pub u64);

/// Whether a rule is currently active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "lowercase")]
pub enum RuleStatus {
    /// Rule is active and will be evaluated.
    Enabled,
    /// Rule exists but is not evaluated.
    Disabled,
    /// Rule is being authored and not yet ready.
    Draft,
}

/// A complete rule definition.
///
/// Rules are the primary unit of automation in Springtale. Each rule has a
/// trigger (when to evaluate), conditions (whether to proceed), and actions
/// (what to do). Rules are authored as TOML files or generated via the
/// NL→Rule parser (Phase 2a).
///
/// No `specta::Type` derive — see the module-level doc comment.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Rule {
    /// Unique identifier.
    #[serde(default = "RuleId::new")]
    pub id: RuleId,

    /// Human-readable name.
    pub name: String,

    /// Optional description of what this rule does.
    #[serde(default)]
    pub description: String,

    /// Whether this rule is active.
    #[serde(default = "default_enabled")]
    pub status: RuleStatus,

    /// Version for tracking updates.
    #[serde(default = "default_version")]
    pub version: RuleVersion,

    /// What event triggers this rule.
    pub trigger: Trigger,

    /// Conditions that must be met for actions to execute.
    /// All conditions must pass (implicit AND at the top level).
    #[serde(default)]
    pub conditions: Vec<Condition>,

    /// Actions to perform when trigger fires and conditions pass.
    pub actions: Vec<Action>,

    /// Cooperation scoping — which agent / formation owns this rule.
    /// Defaults to [`RuleOwner::Global`] for rules created before
    /// Phase 0 (the SQL migration backfills `owner_kind = 'global'`).
    ///
    /// See [`RuleOwner::matches`] for the per-fire filter used by
    /// [`super::engine::RuleEngine::evaluate_with_filter`].
    #[serde(default)]
    pub owner: RuleOwner,
}

/// Cooperation scope for a rule. Rules can be:
///
/// - **Global** — fire from any context. The default for rules
///   created by daemon-queue jobs, system cron, NL parsed rules
///   without an attached agent context, and the existing built-in
///   catalogue (which predates per-bot routing).
/// - **Agent** — fire only when the firing
///   [`crate::rule::engine::TriggerEvent`] is dispatched on behalf
///   of the matching agent.
/// - **Formation** — fire only when the firing context belongs to
///   the matching formation. Cross-formation triggers don't satisfy
///   the filter — a formation A rule never fires for formation B.
///
/// `Uuid` rather than the cooperation-crate `AgentId` /
/// `FormationId` newtypes because `springtale-core` has zero deps on
/// `springtale-cooperation`. Callers in higher crates convert from
/// `AgentId(Uuid)` / `FormationId(Uuid)` at the boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuleOwner {
    /// Any context can fire this rule.
    Global,
    /// Only the named agent can fire this rule.
    Agent { agent_id: Uuid },
    /// Only the named formation can fire this rule.
    Formation { formation_id: Uuid },
}

impl Default for RuleOwner {
    fn default() -> Self {
        Self::Global
    }
}

impl RuleOwner {
    /// `true` when this rule's owner matches a firing context whose
    /// agent / formation ids are `agent_id` / `formation_id`. Global
    /// rules always match. Agent rules match only when the firing
    /// context carries the same agent id. Formation rules match only
    /// when the firing context carries the same formation id.
    pub fn matches(
        &self,
        agent_id: Option<Uuid>,
        formation_id: Option<Uuid>,
    ) -> bool {
        match self {
            RuleOwner::Global => true,
            RuleOwner::Agent { agent_id: rule_agent } => {
                agent_id.is_some_and(|firing| firing == *rule_agent)
            }
            RuleOwner::Formation { formation_id: rule_formation } => {
                formation_id.is_some_and(|firing| firing == *rule_formation)
            }
        }
    }

    /// `true` if the owner is [`RuleOwner::Global`]. Used by storage
    /// readers to short-circuit the per-row JSON parse for the common
    /// case (every rule pre-Phase-0 is Global; the migration backfills
    /// them all).
    pub fn is_global(&self) -> bool {
        matches!(self, RuleOwner::Global)
    }
}

fn default_enabled() -> RuleStatus {
    RuleStatus::Enabled
}

fn default_version() -> RuleVersion {
    RuleVersion(1)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn rule_owner_global_matches_any_context() {
        let owner = RuleOwner::Global;
        assert!(owner.matches(None, None));
        assert!(owner.matches(Some(Uuid::new_v4()), None));
        assert!(owner.matches(None, Some(Uuid::new_v4())));
        assert!(owner.matches(Some(Uuid::new_v4()), Some(Uuid::new_v4())));
    }

    #[test]
    fn rule_owner_agent_requires_matching_agent_id() {
        let agent = Uuid::new_v4();
        let owner = RuleOwner::Agent { agent_id: agent };
        assert!(owner.matches(Some(agent), None));
        assert!(!owner.matches(Some(Uuid::new_v4()), None));
        assert!(!owner.matches(None, None));
        assert!(!owner.matches(None, Some(Uuid::new_v4())));
    }

    #[test]
    fn rule_owner_formation_requires_matching_formation_id() {
        let formation = Uuid::new_v4();
        let owner = RuleOwner::Formation {
            formation_id: formation,
        };
        assert!(owner.matches(None, Some(formation)));
        assert!(!owner.matches(None, Some(Uuid::new_v4())));
        assert!(!owner.matches(None, None));
        assert!(!owner.matches(Some(Uuid::new_v4()), None));
    }

    #[test]
    fn rule_owner_default_is_global() {
        let owner: RuleOwner = Default::default();
        assert!(matches!(owner, RuleOwner::Global));
        assert!(owner.is_global());
    }

    #[test]
    fn rule_owner_round_trips_through_json() {
        let agent_owner = RuleOwner::Agent {
            agent_id: Uuid::new_v4(),
        };
        let s = serde_json::to_string(&agent_owner).unwrap();
        let back: RuleOwner = serde_json::from_str(&s).unwrap();
        assert_eq!(agent_owner, back);

        let formation_owner = RuleOwner::Formation {
            formation_id: Uuid::new_v4(),
        };
        let s = serde_json::to_string(&formation_owner).unwrap();
        let back: RuleOwner = serde_json::from_str(&s).unwrap();
        assert_eq!(formation_owner, back);

        let global = RuleOwner::Global;
        let s = serde_json::to_string(&global).unwrap();
        let back: RuleOwner = serde_json::from_str(&s).unwrap();
        assert_eq!(global, back);
    }

    #[test]
    fn rule_without_owner_field_deserializes_to_global() {
        let toml_without_owner = r#"
name = "test"
[trigger]
type = "Cron"
expression = "0 0 * * *"
[[actions]]
type = "SendMessage"
text = "hi"
"#;
        let rule: Rule = toml::from_str(toml_without_owner).unwrap();
        assert!(matches!(rule.owner, RuleOwner::Global));
    }
}
