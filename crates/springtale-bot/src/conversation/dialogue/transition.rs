//! The slot-filling state machine.
//!
//! [`advance`] fills defaults, finds the next missing required slot, and
//! produces the next thing to say (ask or confirm). [`step`] consumes a
//! user message in an active Collecting/Confirming frame — applying
//! answers, over-answers, and corrections — and returns the action the
//! engine should take. Clarifying is resolved by the engine (it needs
//! the catalogue to pick the chosen recipe); everything else lives here.

use serde_json::Value;

use springtale_runtime::operations::recipes::types::{FieldKind, SelectOption};

use crate::conversation::catalog::{IntentDoc, SlotKindTag, SlotSpec};
use crate::conversation::dialogue::correction;
use crate::conversation::dialogue::frame::{FillSource, FilledSlot, Frame, FrameStep};
use crate::conversation::dialogue::slots::{self, SlotFill};
use crate::conversation::nlg::{Move, SlotPrompt, SummaryLine};
use crate::conversation::nlu::entities;

/// What the engine should do after a continuation turn.
#[derive(Debug)]
pub enum TurnAction {
    /// Render this move, persist the frame, keep the conversation open.
    Speak(Move),
    /// The user confirmed — deploy the recipe, then clear the frame.
    Deploy,
    /// The user backed out — clear the frame and say this.
    Cancel(Move),
}

/// Apply the slot fills extracted from the opening utterance, then
/// produce the first move (ack-wrapped by the engine).
pub fn seed_and_advance(frame: &mut Frame, doc: &IntentDoc, utterance: &str) -> Move {
    for fill in slots::extract_all(utterance, doc) {
        apply_fill(frame, fill);
    }
    advance(frame, doc)
}

/// Fill defaults, find the next missing required slot, set the step, and
/// return the move to speak.
pub fn advance(frame: &mut Frame, doc: &IntentDoc) -> Move {
    fill_defaults(frame, doc);

    if let Some(slot) = first_missing_required(frame, doc) {
        frame.next_slot = Some(slot.id().to_owned());
        frame.step = FrameStep::Collecting;
        Move::Ask {
            slot: slot_prompt(slot),
        }
    } else {
        frame.next_slot = None;
        frame.step = FrameStep::Confirming;
        Move::Confirm {
            lines: summary_lines(frame, doc),
        }
    }
}

/// Continue an active Collecting/Confirming frame.
pub fn step(frame: &mut Frame, doc: &IntentDoc, text: &str) -> TurnAction {
    if entities::is_cancel(text) {
        return TurnAction::Cancel(Move::Cancelled);
    }
    match frame.step {
        FrameStep::Confirming => step_confirming(frame, doc, text),
        // Collecting (and the unreachable Clarifying) both collect input.
        _ => step_collecting(frame, doc, text),
    }
}

fn step_collecting(frame: &mut Frame, doc: &IntentDoc, text: &str) -> TurnAction {
    let mut progressed = false;

    // 1) Direct answer to the slot we asked about.
    if let Some(next_id) = frame.next_slot.clone()
        && let Some(slot) = doc.slot(&next_id)
    {
        match slots::answer_slot(text, slot) {
            Ok(Some(fill)) => {
                apply_fill(frame, fill);
                progressed = true;
            }
            Err(reason) => {
                return TurnAction::Speak(Move::Reask {
                    slot: slot_prompt(slot),
                    reason,
                });
            }
            Ok(None) => {}
        }
    }

    // 2) Over-answers / corrections mentioned in the same message.
    for fill in correction::pending_fills(text, doc, frame) {
        apply_fill(frame, fill);
        progressed = true;
    }

    // 3) Nothing usable → re-ask the current slot.
    if !progressed
        && let Some(next_id) = frame.next_slot.clone()
        && let Some(slot) = doc.slot(&next_id)
    {
        return TurnAction::Speak(Move::Reask {
            slot: slot_prompt(slot),
            reason: "Let me try that again.".to_owned(),
        });
    }

    TurnAction::Speak(advance(frame, doc))
}

fn step_confirming(frame: &mut Frame, doc: &IntentDoc, text: &str) -> TurnAction {
    // Corrections / new values take priority over a bare "yes".
    let pending = correction::pending_fills(text, doc, frame);
    if !pending.is_empty() {
        for fill in pending {
            apply_fill(frame, fill);
        }
        // Re-derive (may re-open Collecting if a cleared slot reappears).
        let mv = advance(frame, doc);
        // If still fully specified, re-confirm rather than re-ask.
        return match mv {
            Move::Confirm { lines } => TurnAction::Speak(Move::Reconfirm { lines }),
            other => TurnAction::Speak(other),
        };
    }

    if entities::is_affirmative(text) {
        return TurnAction::Deploy;
    }

    // The user wants a change but named no concrete value ("actually…",
    // "no, change it") → ask what to change rather than re-dumping the summary.
    if entities::is_negative(text) || correction::is_explicit_change(text) {
        return TurnAction::Speak(Move::AskChange);
    }

    // Unparsed → re-show the summary so they know what's set.
    TurnAction::Speak(Move::Confirm {
        lines: summary_lines(frame, doc),
    })
}

// ── helpers ──────────────────────────────────────────────────────────

fn apply_fill(frame: &mut Frame, fill: SlotFill) {
    frame.fill(
        &fill.slot_id,
        FilledSlot {
            value: fill.value,
            display: fill.display,
            source: fill.source,
        },
    );
}

fn fill_defaults(frame: &mut Frame, doc: &IntentDoc) {
    for slot in &doc.slots {
        if frame.filled.contains_key(slot.id()) {
            continue;
        }
        if let Some(def) = slot.default() {
            frame.fill(
                slot.id(),
                FilledSlot {
                    value: def.clone(),
                    display: default_display(slot, def),
                    source: FillSource::Default,
                },
            );
        }
    }
}

fn first_missing_required<'a>(frame: &Frame, doc: &'a IntentDoc) -> Option<&'a SlotSpec> {
    doc.slots.iter().find(|s| {
        s.is_user_facing()
            && s.is_required()
            // Secrets are never asked in chat (recipes needing them are handed
            // off upstream); skipping here is belt-and-suspenders so a plaintext
            // credential can never reach the session store.
            && s.tag != SlotKindTag::Secret
            && !frame.filled.contains_key(s.id())
    })
}

fn slot_prompt(slot: &SlotSpec) -> SlotPrompt {
    let options = match &slot.field.kind {
        FieldKind::Select { options } => options.iter().map(|o| o.label.clone()).collect(),
        _ => Vec::new(),
    };
    SlotPrompt {
        label: slot.label().to_owned(),
        hint: slot.field.hint.clone(),
        options,
        secret: slot.tag == SlotKindTag::Secret,
    }
}

/// Confirmation lines: every required slot, plus any optional/advanced
/// the user explicitly set (defaults for those stay hidden to keep the
/// summary short). Baked slots never show.
fn summary_lines(frame: &Frame, doc: &IntentDoc) -> Vec<SummaryLine> {
    let mut out = Vec::new();
    for slot in &doc.slots {
        if !slot.is_user_facing() {
            continue;
        }
        let Some(filled) = frame.filled.get(slot.id()) else {
            continue;
        };
        let assumed = filled.source == FillSource::Default;
        if !slot.is_required() && assumed {
            continue;
        }
        out.push(SummaryLine {
            label: slot.label().to_owned(),
            value: filled.display.clone(),
            assumed,
        });
    }
    out
}

/// Display string for a default value — the option label for a `Select`,
/// a humanized time for a recognizable cron, else the raw scalar.
fn default_display(slot: &SlotSpec, value: &Value) -> String {
    if let FieldKind::Select { options } = &slot.field.kind
        && let Value::String(v) = value
        && let Some(o) = options.iter().find(|o: &&SelectOption| &o.value == v)
    {
        return o.label.clone();
    }
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::conversation::catalog::CatalogSnapshot;
    use crate::conversation::nlg;
    use springtale_runtime::operations::recipes::types::{
        Difficulty, FieldVisibility, InputField, Recipe, RecipeBlueprint, RecipeCategory,
        RecipeSource,
    };

    fn weather_recipe(loc_default: Option<&str>, sched_default: Option<&str>) -> Recipe {
        Recipe {
            id: "w".into(),
            name: "Morning Weather".into(),
            description: "weather".into(),
            icon_id: "x".into(),
            category: RecipeCategory::Daily,
            tags: vec![],
            connectors_used: vec![],
            ai_required: false,
            difficulty: Difficulty::Quick,
            source: RecipeSource::Builtin,
            inputs: vec![
                InputField {
                    id: "location".into(),
                    label: "City".into(),
                    kind: FieldKind::Select {
                        options: vec![
                            SelectOption {
                                value: "lat=phx".into(),
                                label: "Phoenix".into(),
                            },
                            SelectOption {
                                value: "lat=tus".into(),
                                label: "Tucson".into(),
                            },
                        ],
                    },
                    visibility: FieldVisibility::Required,
                    default: loc_default.map(|v| serde_json::json!(v)),
                    hint: None,
                },
                InputField {
                    id: "schedule".into(),
                    label: "Time of day".into(),
                    kind: FieldKind::Select {
                        options: vec![
                            SelectOption {
                                value: "0 7 * * *".into(),
                                label: "7:00 AM".into(),
                            },
                            SelectOption {
                                value: "0 8 * * *".into(),
                                label: "8:00 AM".into(),
                            },
                        ],
                    },
                    visibility: FieldVisibility::Required,
                    default: sched_default.map(|v| serde_json::json!(v)),
                    hint: None,
                },
            ],
            blueprint: RecipeBlueprint {
                connector_configs: vec![],
                rules: vec![],
                ai_config: None,
                summary: None,
                derived_inputs: vec![],
            },
        }
    }

    fn doc(recipe: Recipe) -> IntentDoc {
        CatalogSnapshot::build(vec![recipe])
            .find("w")
            .unwrap()
            .clone()
    }

    #[test]
    fn test_one_shot_with_defaults_goes_straight_to_confirm() {
        let d = doc(weather_recipe(Some("lat=phx"), Some("0 8 * * *")));
        let mut frame = Frame::collecting("w", "set up morning weather", chrono::Utc::now());
        let mv = seed_and_advance(&mut frame, &d, "set up morning weather");
        assert!(matches!(mv, Move::Confirm { .. }));
        assert_eq!(frame.step, FrameStep::Confirming);
    }

    #[test]
    fn test_missing_required_without_default_is_asked() {
        let d = doc(weather_recipe(None, None));
        let mut frame = Frame::collecting("w", "set up weather", chrono::Utc::now());
        let mv = seed_and_advance(&mut frame, &d, "set up weather");
        assert!(matches!(mv, Move::Ask { .. }));
        assert_eq!(frame.next_slot.as_deref(), Some("location"));
    }

    #[test]
    fn test_full_collect_then_confirm_then_deploy() {
        let d = doc(weather_recipe(None, None));
        let mut frame = Frame::collecting("w", "weather", chrono::Utc::now());
        // seed finds nothing → asks location
        let _ = seed_and_advance(&mut frame, &d, "weather");
        assert_eq!(frame.next_slot.as_deref(), Some("location"));
        // answer location → asks schedule
        let a = step(&mut frame, &d, "tucson");
        assert!(matches!(a, TurnAction::Speak(Move::Ask { .. })));
        assert_eq!(frame.next_slot.as_deref(), Some("schedule"));
        // answer schedule → confirm
        let b = step(&mut frame, &d, "8am");
        assert!(matches!(b, TurnAction::Speak(Move::Confirm { .. })));
        // confirm → deploy
        let c = step(&mut frame, &d, "yes");
        assert!(matches!(c, TurnAction::Deploy));
    }

    #[test]
    fn test_confirm_negative_without_value_asks_what_to_change() {
        let d = doc(weather_recipe(Some("lat=phx"), Some("0 8 * * *")));
        let mut frame = Frame::collecting("w", "morning weather", chrono::Utc::now());
        let _ = seed_and_advance(&mut frame, &d, "morning weather");
        assert_eq!(frame.step, FrameStep::Confirming);
        // "no, change it" names no concrete value → ask what to change.
        let a = step(&mut frame, &d, "no, change it");
        assert!(matches!(a, TurnAction::Speak(Move::AskChange)));
        // Still in Confirming — the user can now name the new value.
        let b = step(&mut frame, &d, "tucson");
        assert!(matches!(b, TurnAction::Speak(Move::Reconfirm { .. })));
        assert_eq!(frame.filled.get("location").unwrap().display, "Tucson");
    }

    #[test]
    fn test_correction_during_confirm_reconfirms() {
        let d = doc(weather_recipe(Some("lat=phx"), Some("0 8 * * *")));
        let mut frame = Frame::collecting("w", "morning weather", chrono::Utc::now());
        let _ = seed_and_advance(&mut frame, &d, "morning weather");
        assert_eq!(frame.step, FrameStep::Confirming);
        let a = step(&mut frame, &d, "actually make it tucson at 7am");
        assert!(matches!(a, TurnAction::Speak(Move::Reconfirm { .. })));
        assert_eq!(frame.filled.get("location").unwrap().display, "Tucson");
        assert_eq!(frame.filled.get("schedule").unwrap().display, "7:00 AM");
    }

    #[test]
    fn test_cancel_anytime() {
        let d = doc(weather_recipe(None, None));
        let mut frame = Frame::collecting("w", "weather", chrono::Utc::now());
        let _ = seed_and_advance(&mut frame, &d, "weather");
        let a = step(&mut frame, &d, "never mind");
        assert!(matches!(a, TurnAction::Cancel(_)));
    }

    #[test]
    fn test_confirm_summary_marks_assumed_defaults() {
        let d = doc(weather_recipe(Some("lat=phx"), Some("0 8 * * *")));
        let mut frame = Frame::collecting("w", "weather in tucson", chrono::Utc::now());
        // user named tucson but not time → time is an assumed default
        let mv = seed_and_advance(&mut frame, &d, "weather in tucson");
        let text = nlg::render(&mv, 0);
        assert!(text.contains("Tucson"));
        assert!(text.contains("default")); // schedule assumed
    }
}
