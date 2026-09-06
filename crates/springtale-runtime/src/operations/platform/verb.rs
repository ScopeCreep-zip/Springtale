//! The platform-verb descriptor.
//!
//! One value per thing chat is allowed to ask the platform to do. The
//! same registry backs the chat command handlers, the NLU intent
//! documents, and (later, plan 2.3) the AI tool list, so a verb that
//! exists on one surface exists on all of them with the same name,
//! description, schema, and read-only classification.

use serde::Serialize;
use serde_json::Value;

/// Which of the orchestration groups a verb belongs to.
///
/// The drum rule (`docs/intended-arch/COOPERATION.pdf`): you steer a
/// formation, you never hand work to a named member. That is why there
/// is no `Assignment` variant — composition adds and removes members,
/// it does not give one of them a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VerbGroup {
    /// Read-only. Never needs approval.
    Inspection,
    /// Who is in the formation (add / remove a member).
    Composition,
    /// What the formation is trying to do.
    Intent,
    /// What the formation is allowed to do (guard, safety, model).
    Constraints,
    /// Direct intervention in a running formation.
    Intervention,
}

impl VerbGroup {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Inspection => "inspection",
            Self::Composition => "composition",
            Self::Intent => "intent",
            Self::Constraints => "constraints",
            Self::Intervention => "intervention",
        }
    }
}

/// One platform verb, as every surface sees it.
#[derive(Debug, Clone, Serialize)]
pub struct PlatformVerb {
    /// Dotted name, e.g. `formation.pause`. The chat command is the
    /// segment before the dot; the sub-command is the segment after.
    pub name: &'static str,
    /// One line, shown in `/help` and used as the AI tool description.
    pub description: &'static str,
    /// The orchestration group this verb belongs to.
    pub group: VerbGroup,
    /// True only when the verb purely retrieves state. Read-only verbs
    /// run without an approval; everything else goes through the same
    /// gate as a connector write.
    pub read_only: bool,
    /// Names of the arguments the verb takes, in order. `formation`
    /// means "a formation name" and is the slot the NLU gazetteer fills
    /// from the live formation list.
    pub args: &'static [&'static str],
}

impl PlatformVerb {
    /// Chat command name — `formation.pause` → `formation`.
    pub fn command(&self) -> &'static str {
        match self.name.split_once('.') {
            Some((head, _)) => head,
            None => self.name,
        }
    }

    /// Sub-command — `formation.pause` → `pause`.
    pub fn sub(&self) -> &'static str {
        match self.name.split_once('.') {
            Some((_, tail)) => tail,
            None => "",
        }
    }

    /// True when the verb's first argument is a formation name.
    pub fn takes_formation(&self) -> bool {
        self.args.first() == Some(&"formation")
    }

    /// JSON Schema for the verb's arguments — what the AI tool list
    /// (plan 2.3) publishes and what the contract check reads.
    pub fn input_schema(&self) -> Value {
        let mut props = serde_json::Map::new();
        for arg in self.args {
            props.insert(
                (*arg).to_owned(),
                serde_json::json!({ "type": "string", "description": arg }),
            );
        }
        serde_json::json!({
            "type": "object",
            "properties": Value::Object(props),
            "required": self.args,
        })
    }
}
