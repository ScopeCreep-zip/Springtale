//! Slot extraction — pull recipe input values out of free text.
//!
//! Two entry points:
//!   - [`extract_all`] scans an opening utterance for every slot it can
//!     resolve (gazetteer + grammar), skipping `Secret`/`Text` (never
//!     scrape a password or free prose mid-sentence).
//!   - [`answer_slot`] interprets a reply as the answer to ONE slot the
//!     dialogue just asked about (numbered pick, label, raw text…).
//!
//! Every value is validated against its `FieldKind` (reusing the
//! runtime's `validate_kind`) before it's accepted.

use serde_json::Value;

use springtale_runtime::operations::preflight::checks::validate_kind;
use springtale_runtime::operations::recipes::types::{FieldKind, SelectOption};

use crate::conversation::catalog::{IntentDoc, SlotKindTag, SlotSpec};
use crate::conversation::dialogue::frame::FillSource;
use crate::conversation::nlu::entities;

/// A resolved slot value ready to store in the frame.
#[derive(Debug, Clone)]
pub struct SlotFill {
    pub slot_id: String,
    pub value: Value,
    pub display: String,
    pub source: FillSource,
}

/// Scan an opening utterance for every slot we can pre-fill.
/// Skips `Secret`/`Text` (explicit-prompt only) and `Baked`.
pub fn extract_all(text: &str, doc: &IntentDoc) -> Vec<SlotFill> {
    let mut out = Vec::new();
    for slot in &doc.slots {
        if !slot.is_user_facing() {
            continue;
        }
        match slot.tag {
            SlotKindTag::Secret | SlotKindTag::Text | SlotKindTag::Other => continue,
            _ => {}
        }
        if let Some(fill) = extract_one(text, slot, FillSource::Grammar) {
            out.push(fill);
        }
    }
    out
}

/// Interpret `text` as the answer to a specific slot the bot just asked
/// for. Returns `Err(reason)` when the answer is present but invalid
/// (so the dialogue can re-ask), `Ok(None)` when nothing usable was
/// found, `Ok(Some(fill))` on success.
pub fn answer_slot(text: &str, slot: &SlotSpec) -> Result<Option<SlotFill>, String> {
    let trimmed = text.trim();

    match slot.tag {
        // A direct answer to a free-text or place prompt is the whole reply
        // (when asked "which city?", "Sacramento, CA" is the answer).
        SlotKindTag::Text | SlotKindTag::Place => {
            if trimmed.is_empty() {
                return Ok(None);
            }
            Ok(Some(SlotFill {
                slot_id: slot.id().to_owned(),
                value: Value::String(trimmed.to_owned()),
                display: trimmed.to_owned(),
                source: FillSource::UserPrompt,
            }))
        }
        // Secrets are NEVER collected in chat — a plaintext credential must
        // not land in the (unencrypted) session store. Recipes that need one
        // are handed off to the secure setup flow before any frame starts, so
        // this arm is unreachable; returning `None` guarantees no secret is
        // ever stored even if a future caller reaches it.
        SlotKindTag::Secret => Ok(None),
        SlotKindTag::Select => {
            // Numbered pick ("1", "2") first.
            if let Some(opts) = select_options(slot)
                && let Ok(n) = trimmed.parse::<usize>()
                && n >= 1
                && n <= opts.len()
            {
                let o = &opts[n - 1];
                return Ok(Some(select_fill(slot, o, FillSource::UserPrompt)));
            }
            Ok(extract_one(text, slot, FillSource::UserPrompt))
        }
        SlotKindTag::Cron | SlotKindTag::Url | SlotKindTag::Number | SlotKindTag::Bool => {
            match extract_one(text, slot, FillSource::UserPrompt) {
                Some(f) => Ok(Some(f)),
                None => Ok(None),
            }
        }
        SlotKindTag::Other => Ok(None),
    }
}

/// Core per-slot extraction shared by both entry points.
fn extract_one(text: &str, slot: &SlotSpec, source: FillSource) -> Option<SlotFill> {
    match slot.tag {
        SlotKindTag::Select => extract_select(text, slot, source),
        SlotKindTag::Cron => {
            let cron = entities::parse_schedule(text)?;
            accept(
                slot,
                Value::String(cron.clone()),
                humanize_cron(&cron),
                source,
            )
        }
        SlotKindTag::Url => {
            let url = entities::parse_url(text)?;
            accept(slot, Value::String(url.clone()), url, source)
        }
        SlotKindTag::Number => {
            let n = entities::parse_number(text)?;
            accept(slot, Value::Number(n.into()), n.to_string(), source)
        }
        SlotKindTag::Bool => {
            let b = entities::parse_bool(text)?;
            accept(
                slot,
                Value::Bool(b),
                if b { "on" } else { "off" }.to_owned(),
                source,
            )
        }
        SlotKindTag::Place => {
            let place = entities::parse_place(text)?;
            accept(slot, Value::String(place.clone()), place, source)
        }
        SlotKindTag::Text | SlotKindTag::Secret | SlotKindTag::Other => None,
    }
}

/// `Select` extraction: gazetteer label hit, or — for cron-valued
/// option sets (schedule presets) — a parsed time snapped to the
/// nearest preset.
fn extract_select(text: &str, slot: &SlotSpec, source: FillSource) -> Option<SlotFill> {
    if let Some(g) = &slot.gazetteer
        && let Some(hit) = g.match_in(text)
    {
        return Some(SlotFill {
            slot_id: slot.id().to_owned(),
            value: Value::String(hit.value),
            display: hit.label,
            source,
        });
    }

    // Cron-valued select (e.g. a schedule field): snap a parsed time.
    let opts = select_options(slot)?;
    if is_cron_select(opts)
        && let Some(time) = entities::parse_time(text)
        && let Some(o) = snap_time(opts, time.hour)
    {
        return Some(select_fill(slot, o, source));
    }
    None
}

fn select_fill(slot: &SlotSpec, opt: &SelectOption, source: FillSource) -> SlotFill {
    SlotFill {
        slot_id: slot.id().to_owned(),
        value: Value::String(opt.value.clone()),
        display: opt.label.clone(),
        source,
    }
}

/// Validate a grammar-produced value against the slot kind; drop it on
/// failure (treated as "still missing").
fn accept(slot: &SlotSpec, value: Value, display: String, source: FillSource) -> Option<SlotFill> {
    validate_kind(&slot.field.kind, &value).ok()?;
    Some(SlotFill {
        slot_id: slot.id().to_owned(),
        value,
        display,
        source,
    })
}

fn select_options(slot: &SlotSpec) -> Option<&[SelectOption]> {
    match &slot.field.kind {
        FieldKind::Select { options } => Some(options),
        _ => None,
    }
}

fn is_cron_select(opts: &[SelectOption]) -> bool {
    !opts.is_empty() && opts.iter().all(|o| entities::cron_hour(&o.value).is_some())
}

fn snap_time(opts: &[SelectOption], hour: u8) -> Option<&SelectOption> {
    opts.iter().min_by_key(|o| {
        entities::cron_hour(&o.value)
            .map(|h| h.abs_diff(hour))
            .unwrap_or(u8::MAX)
    })
}

/// Render a simple `M H * * *` cron as a friendly time for confirmations.
fn humanize_cron(expr: &str) -> String {
    let fields: Vec<&str> = expr.split_whitespace().collect();
    if fields.len() == 5
        && let (Ok(min), Ok(hour)) = (fields[0].parse::<u8>(), fields[1].parse::<u8>())
    {
        let (h12, ap) = match hour {
            0 => (12, "AM"),
            1..=11 => (hour, "AM"),
            12 => (12, "PM"),
            _ => (hour - 12, "PM"),
        };
        let when = match (fields[2], fields[3], fields[4]) {
            ("*", "*", "*") => "every day".to_owned(),
            ("*", "*", "1-5") => "weekdays".to_owned(),
            _ => "on schedule".to_owned(),
        };
        return format!("{when} at {h12}:{min:02} {ap}");
    }
    expr.to_owned()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::conversation::catalog::CatalogSnapshot;
    use springtale_runtime::operations::recipes::types::{
        Difficulty, FieldVisibility, InputField, Recipe, RecipeBlueprint, RecipeCategory,
        RecipeSource,
    };

    fn weather() -> Recipe {
        Recipe {
            id: "weather-briefing".into(),
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
                    default: None,
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
                            SelectOption {
                                value: "0 9 * * *".into(),
                                label: "9:00 AM".into(),
                            },
                        ],
                    },
                    visibility: FieldVisibility::Required,
                    default: None,
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

    fn doc() -> IntentDoc {
        CatalogSnapshot::build(vec![weather()])
            .find("weather-briefing")
            .unwrap()
            .clone()
    }

    #[test]
    fn test_extract_all_fills_city_and_time_from_one_sentence() {
        let fills = extract_all("weather in Tucson every morning", &doc());
        let loc = fills.iter().find(|f| f.slot_id == "location").unwrap();
        assert_eq!(loc.value, serde_json::json!("lat=tus"));
        let sched = fills.iter().find(|f| f.slot_id == "schedule").unwrap();
        // "every morning" → 8:00 AM preset
        assert_eq!(sched.value, serde_json::json!("0 8 * * *"));
        assert_eq!(sched.display, "8:00 AM");
    }

    #[test]
    fn test_snap_time_to_nearest_preset() {
        // 9am exact preset exists
        let fills = extract_all("weather in phoenix at 9am", &doc());
        let sched = fills.iter().find(|f| f.slot_id == "schedule").unwrap();
        assert_eq!(sched.value, serde_json::json!("0 9 * * *"));
    }

    #[test]
    fn test_answer_select_by_number() {
        let d = doc();
        let slot = d.slot("location").unwrap();
        let fill = answer_slot("2", slot).unwrap().unwrap();
        assert_eq!(fill.value, serde_json::json!("lat=tus")); // 2nd option = Tucson
    }

    #[test]
    fn test_answer_select_by_label() {
        let d = doc();
        let slot = d.slot("location").unwrap();
        let fill = answer_slot("phoenix please", slot).unwrap().unwrap();
        assert_eq!(fill.display, "Phoenix");
    }
}
