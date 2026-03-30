use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::action::Action;
use super::condition::Condition;
use super::trigger::Trigger;

/// Unique identifier for a rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
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
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
pub struct RuleVersion(pub u64);

/// Whether a rule is currently active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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
}

fn default_enabled() -> RuleStatus {
    RuleStatus::Enabled
}

fn default_version() -> RuleVersion {
    RuleVersion(1)
}
