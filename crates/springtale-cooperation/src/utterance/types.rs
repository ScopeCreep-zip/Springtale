//! Utterance wire types. Shared by the bus (`BroadcastTrigger::Utterance`),
//! the observer stream (`CooperationEvent::Utterance`) and the canvas.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::cadence::AgentId;
use crate::tick::TickId;
use crate::types::FormationId;

/// What was said. The `utter` tag keeps the inner kind flat under the
/// observer event's `kind` tag.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "utter", rename_all = "snake_case")]
pub enum UtteranceKind {
    Firing,
    Working,
    Listening,
    Idle,
    Failed,
    Down,
    Claimed { task: uuid::Uuid },
    Yield { beneficiary: AgentId },
    Helping { target: AgentId },
    Rally,
    Cascade { streak: u32 },
}

impl UtteranceKind {
    /// Stable def-table key. Also the `(agent, name)` block key.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Firing => "firing",
            Self::Working => "working",
            Self::Listening => "listening",
            Self::Idle => "idle",
            Self::Failed => "failed",
            Self::Down => "down",
            Self::Claimed { .. } => "claimed",
            Self::Yield { .. } => "yield",
            Self::Helping { .. } => "helping",
            Self::Rally => "rally",
            Self::Cascade { .. } => "cascade",
        }
    }
}

/// Cohn's carrier. `Speech` and `Burst` are perceived by the formation;
/// `Thought` is not. Drawn as border style: solid for speech and burst,
/// dotted for thought.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum Carrier {
    Speech,
    Burst,
    Thought,
    None,
}

impl Carrier {
    /// Whether peers in the formation perceive this carrier (Cohn 2013).
    pub fn heard_by_peers(self) -> bool {
        matches!(self, Self::Speech | Self::Burst)
    }
}

/// ISO 3864 severity shape. Colour is never the only carrier of state
/// (WCAG 1.4.1): triangle = warning, circle = prohibition / stop,
/// square = information.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum Shape {
    Triangle,
    Circle,
    Square,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum Tone {
    Calm,
    Alert,
    Urgent,
}

impl Tone {
    /// Severity for the existing `comms::StateMessage.severity` field.
    pub fn severity(self) -> f32 {
        match self {
            Self::Calm => 0.2,
            Self::Alert => 0.6,
            Self::Urgent => 0.9,
        }
    }
}

/// One thing said, resolved against the def table at the event site.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct Utterance {
    pub formation_id: Option<FormationId>,
    /// Formation member. `None` for solo rules (then `rule_id` is set)
    /// and formation-level kinds.
    pub agent: Option<AgentId>,
    pub rule_id: Option<springtale_core::rule::RuleId>,
    pub utterance: UtteranceKind,
    pub carrier: Carrier,
    pub shape: Shape,
    pub tone: Tone,
    /// Tick it was said on. Renderers expire against the colony timeline,
    /// never wall-clock.
    pub seq: TickId,
    pub ttl_ticks: u32,
    /// Default-locale glyph frames from the def table, each a string of
    /// codepoints present in the shipped symbol font.
    pub glyph_frames: Vec<String>,
    /// Flip horizontally under RTL locales (directional glyphs only).
    pub mirror_rtl: bool,
    /// i18n key for the text alternative, present in every locale dictionary.
    pub label_key: String,
}
