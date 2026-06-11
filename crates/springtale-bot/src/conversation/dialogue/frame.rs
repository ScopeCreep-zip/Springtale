//! The slot-filling dialogue frame — the FSM state persisted between
//! turns inside `Session.state_data["conversation"]`.
//!
//! A frame is created when the engine recognizes a setup request and
//! lives (with a TTL) until the user confirms, cancels, or abandons it.
//! It records which recipe is being configured, every slot value
//! gathered so far (with where each came from), and where the
//! conversation is in the collect → confirm flow.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use springtale_runtime::operations::recipes::types::RecipeInputs;

use crate::state::session::Session;

/// Key under which the frame is stored in `Session.state_data`.
const FRAME_KEY: &str = "conversation";
/// Frame lifetime in minutes — a stale half-finished setup is silently
/// dropped rather than resumed days later.
const FRAME_TTL_MINUTES: i64 = 15;
/// Current frame schema version (bump on a breaking shape change).
const FRAME_VERSION: u8 = 1;

/// Where in the setup flow the dialogue is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameStep {
    /// Multiple recipes matched — waiting for the user to pick one.
    Clarifying,
    /// Gathering slot values.
    Collecting,
    /// All required slots filled — waiting for a yes/no.
    Confirming,
}

/// Provenance of a filled slot — drives "this is my assumption" phrasing
/// and correction handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FillSource {
    /// Matched an option label in the opening utterance.
    Gazetteer,
    /// Parsed by a grammar extractor (time, url, number…).
    Grammar,
    /// Filled from the recipe's declared default (an assumption).
    Default,
    /// The user typed it in answer to a direct prompt.
    UserPrompt,
}

/// One gathered slot value.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilledSlot {
    /// Stored value (the `Select` option value, cron string, etc.) —
    /// exactly what goes into `RecipeInputs`.
    pub value: serde_json::Value,
    /// Human-facing display (the `Select` label, masked secret…).
    pub display: String,
    pub source: FillSource,
}

/// The dialogue frame.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Frame {
    pub v: u8,
    pub step: FrameStep,
    /// `None` only while `Clarifying`.
    pub recipe_id: Option<String>,
    /// Candidate recipe ids offered during `Clarifying`.
    #[serde(default)]
    pub candidates: Vec<String>,
    /// The message that started the frame — re-extracted after the user
    /// disambiguates so a one-shot sentence still pre-fills slots.
    pub original_utterance: String,
    pub filled: BTreeMap<String, FilledSlot>,
    pub next_slot: Option<String>,
    pub created_at: String,
    pub expires_at: String,
    /// Monotonic per-frame counter that varies NLG phrasing each turn.
    #[serde(default)]
    pub seq: u64,
}

impl Frame {
    fn new(step: FrameStep, now: chrono::DateTime<chrono::Utc>) -> Self {
        let expires = now + chrono::Duration::minutes(FRAME_TTL_MINUTES);
        Self {
            v: FRAME_VERSION,
            step,
            recipe_id: None,
            candidates: Vec::new(),
            original_utterance: String::new(),
            filled: BTreeMap::new(),
            next_slot: None,
            created_at: now.to_rfc3339(),
            expires_at: expires.to_rfc3339(),
            seq: 0,
        }
    }

    /// Start collecting for a known recipe.
    pub fn collecting(
        recipe_id: &str,
        utterance: &str,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Self {
        let mut f = Self::new(FrameStep::Collecting, now);
        f.recipe_id = Some(recipe_id.to_owned());
        f.original_utterance = utterance.to_owned();
        f
    }

    /// Start a clarification between several candidate recipes.
    pub fn clarifying(
        candidates: Vec<String>,
        utterance: &str,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Self {
        let mut f = Self::new(FrameStep::Clarifying, now);
        f.candidates = candidates;
        f.original_utterance = utterance.to_owned();
        f
    }

    pub fn is_expired(&self, now: chrono::DateTime<chrono::Utc>) -> bool {
        chrono::DateTime::parse_from_rfc3339(&self.expires_at)
            .map(|exp| now >= exp.with_timezone(&chrono::Utc))
            .unwrap_or(true)
    }

    /// Set or overwrite a slot value.
    pub fn fill(&mut self, slot_id: &str, slot: FilledSlot) {
        self.filled.insert(slot_id.to_owned(), slot);
    }

    /// Build the `RecipeInputs` to hand to the deployer.
    pub fn to_recipe_inputs(&self) -> RecipeInputs {
        let mut inputs = RecipeInputs::empty();
        for (id, slot) in &self.filled {
            inputs.insert(id.clone(), slot.value.clone());
        }
        inputs
    }

    /// Advance the NLG variation counter.
    pub fn bump_seq(&mut self) {
        self.seq = self.seq.wrapping_add(1);
    }

    // ── persistence into Session.state_data ──────────────────────────

    /// Read the active (non-expired) frame from a session, if any.
    pub fn load(session: &Session, now: chrono::DateTime<chrono::Utc>) -> Option<Frame> {
        let raw = session.state_data.get(FRAME_KEY)?;
        let frame: Frame = serde_json::from_value(raw.clone()).ok()?;
        if frame.v != FRAME_VERSION || frame.is_expired(now) {
            return None;
        }
        Some(frame)
    }

    /// Write this frame into the session's state.
    pub fn store_into(&self, session: &mut Session) {
        if let Ok(value) = serde_json::to_value(self) {
            set_state_key(session, FRAME_KEY, value);
        }
    }

    /// Remove any frame from the session (on deploy / cancel).
    pub fn clear(session: &mut Session) {
        if let serde_json::Value::Object(map) = &mut session.state_data {
            map.remove(FRAME_KEY);
        }
    }
}

fn set_state_key(session: &mut Session, key: &str, value: serde_json::Value) {
    match &mut session.state_data {
        serde_json::Value::Object(map) => {
            map.insert(key.to_owned(), value);
        }
        other => {
            let mut map = serde_json::Map::new();
            map.insert(key.to_owned(), value);
            *other = serde_json::Value::Object(map);
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn now() -> chrono::DateTime<chrono::Utc> {
        chrono::Utc::now()
    }

    #[test]
    fn test_frame_round_trips_through_session() {
        let mut session = Session::new("u", "c");
        let mut f = Frame::collecting("weather-briefing", "weather in tucson", now());
        f.fill(
            "location",
            FilledSlot {
                value: serde_json::json!("lat=tus"),
                display: "Tucson".into(),
                source: FillSource::Gazetteer,
            },
        );
        f.store_into(&mut session);

        let loaded = Frame::load(&session, now()).unwrap();
        assert_eq!(loaded.recipe_id.as_deref(), Some("weather-briefing"));
        assert_eq!(loaded.filled.get("location").unwrap().display, "Tucson");
    }

    #[test]
    fn test_expired_frame_is_not_loaded() {
        let mut session = Session::new("u", "c");
        let past = now() - chrono::Duration::minutes(30);
        let f = Frame::collecting("x", "y", past);
        f.store_into(&mut session);
        assert!(Frame::load(&session, now()).is_none());
    }

    #[test]
    fn test_clear_removes_frame() {
        let mut session = Session::new("u", "c");
        let f = Frame::collecting("x", "y", now());
        f.store_into(&mut session);
        Frame::clear(&mut session);
        assert!(Frame::load(&session, now()).is_none());
    }

    #[test]
    fn test_to_recipe_inputs() {
        let mut f = Frame::collecting("x", "y", now());
        f.fill(
            "location",
            FilledSlot {
                value: serde_json::json!("lat=tus"),
                display: "Tucson".into(),
                source: FillSource::Gazetteer,
            },
        );
        let inputs = f.to_recipe_inputs();
        assert_eq!(
            inputs.get("location").unwrap(),
            &serde_json::json!("lat=tus")
        );
    }
}
