use serde::{Deserialize, Serialize};

use super::action::Action;
use super::condition::Condition;
use super::trigger::Trigger;

/// A pre-built rule pattern (IFTTT-style recipe).
///
/// Templates provide common automation patterns with configurable parameters.
/// Users customize parameters, not logic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleTemplate {
    /// Template name (e.g., "stream-announce", "download-organizer").
    pub name: String,

    /// Human-readable description.
    pub description: String,

    /// Category for grouping (e.g., "streaming", "files", "social").
    pub category: String,

    /// The trigger pattern.
    pub trigger: Trigger,

    /// Default conditions (user can modify).
    pub conditions: Vec<Condition>,

    /// Default actions (user can modify).
    pub actions: Vec<Action>,

    /// Parameters the user must fill in.
    pub parameters: Vec<TemplateParameter>,
}

/// A parameter in a rule template that the user must provide.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateParameter {
    /// Parameter name (used in template variable substitution).
    pub name: String,

    /// Human-readable label.
    pub label: String,

    /// Description of what this parameter does.
    pub description: String,

    /// Default value (if any).
    pub default: Option<String>,

    /// Whether this parameter is required.
    pub required: bool,
}
