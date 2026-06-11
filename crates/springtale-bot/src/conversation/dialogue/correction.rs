//! Mid-flow correction detection.
//!
//! Lets a user change their mind without starting over — "actually make
//! it 9am", "no, Tucson". Works deterministically: re-run extraction on
//! the new message and keep any fill that's new or differs from what the
//! frame already holds. The explicit-keyword check is a secondary signal
//! used when the user expresses a change but names no concrete value
//! yet ("change the time").

use crate::conversation::catalog::IntentDoc;
use crate::conversation::dialogue::frame::Frame;
use crate::conversation::dialogue::slots::{self, SlotFill};

/// Fills from `text` that should be applied to `frame` — values that are
/// either new or different from what's stored. Same-value re-mentions are
/// dropped (no-op).
pub fn pending_fills(text: &str, doc: &IntentDoc, frame: &Frame) -> Vec<SlotFill> {
    slots::extract_all(text, doc)
        .into_iter()
        .filter(|f| {
            frame
                .filled
                .get(&f.slot_id)
                .map(|existing| existing.value != f.value)
                .unwrap_or(true)
        })
        .collect()
}

/// Did the user signal a change in words, even if no concrete value was
/// parsed? Used to offer a "what would you like to change?" nudge.
pub fn is_explicit_change(text: &str) -> bool {
    let lower = text.to_lowercase();
    [
        "actually",
        "change",
        "instead",
        "rather",
        "different",
        "make it",
        "not ",
    ]
    .iter()
    .any(|kw| lower.contains(kw))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::conversation::catalog::CatalogSnapshot;
    use crate::conversation::dialogue::frame::{FillSource, FilledSlot};
    use springtale_runtime::operations::recipes::types::{
        Difficulty, FieldKind, FieldVisibility, InputField, Recipe, RecipeBlueprint,
        RecipeCategory, RecipeSource, SelectOption,
    };

    fn doc() -> IntentDoc {
        let recipe = Recipe {
            id: "w".into(),
            name: "Weather".into(),
            description: "w".into(),
            icon_id: "x".into(),
            category: RecipeCategory::Daily,
            tags: vec![],
            connectors_used: vec![],
            ai_required: false,
            difficulty: Difficulty::Quick,
            source: RecipeSource::Builtin,
            inputs: vec![InputField {
                id: "schedule".into(),
                label: "Time".into(),
                kind: FieldKind::Select {
                    options: vec![
                        SelectOption {
                            value: "0 7 * * *".into(),
                            label: "7:00 AM".into(),
                        },
                        SelectOption {
                            value: "0 9 * * *".into(),
                            label: "9:00 AM".into(),
                        },
                    ],
                },
                visibility: FieldVisibility::Required,
                default: None,
                hint: None,
            }],
            blueprint: RecipeBlueprint {
                connector_configs: vec![],
                rules: vec![],
                ai_config: None,
                summary: None,
                derived_inputs: vec![],
            },
        };
        CatalogSnapshot::build(vec![recipe])
            .find("w")
            .unwrap()
            .clone()
    }

    #[test]
    fn test_pending_fills_detects_changed_value() {
        let d = doc();
        let mut frame = Frame::collecting("w", "", chrono::Utc::now());
        frame.fill(
            "schedule",
            FilledSlot {
                value: serde_json::json!("0 7 * * *"),
                display: "7:00 AM".into(),
                source: FillSource::Grammar,
            },
        );
        let fills = pending_fills("actually make it 9am", &d, &frame);
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].value, serde_json::json!("0 9 * * *"));
    }

    #[test]
    fn test_same_value_is_noop() {
        let d = doc();
        let mut frame = Frame::collecting("w", "", chrono::Utc::now());
        frame.fill(
            "schedule",
            FilledSlot {
                value: serde_json::json!("0 9 * * *"),
                display: "9:00 AM".into(),
                source: FillSource::Grammar,
            },
        );
        assert!(pending_fills("9am is fine", &d, &frame).is_empty());
    }
}
