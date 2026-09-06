//! Platform verbs projected as intent documents (plan 5.4).
//!
//! The NLU already scores recipes as [`IntentDoc`]s; the platform verbs
//! become documents of the same shape, so "hold the research squad"
//! ranks against `formation.pause` exactly the way "morning weather"
//! ranks against a recipe. Two things differ from a recipe document:
//! the token bag is phrased from the per-locale sentence templates
//! rather than a recipe name, and the `{formation}` slot's gazetteer is
//! built from the live formation list at match time — never hard-coded.

use springtale_runtime::operations::platform::{PlatformVerb, platform_verbs};
use springtale_runtime::operations::recipes::types::{
    FieldKind, FieldVisibility, InputField, RecipeCategory, SelectOption,
};

use super::snapshot::{IntentDoc, SlotKindTag, SlotSpec};
use crate::conversation::nlu::gazetteer::Gazetteer;
use crate::conversation::nlu::normalize::tokenize;
use crate::conversation::sentences;

/// Document id prefix, so a platform document can never collide with a
/// recipe id and `IntentDoc::platform_verb` is the only thing routing
/// reads.
pub const PLATFORM_PREFIX: &str = "platform:";

/// Build one [`IntentDoc`] per platform verb.
///
/// `formation_names` is the live roster read from the store at match
/// time; it fills the `{formation}` slot's gazetteer.
pub fn platform_docs(locale: &str, formation_names: &[String]) -> Vec<IntentDoc> {
    let catalog = sentences::for_locale(locale);
    platform_verbs()
        .iter()
        .map(|v| project_verb(v, catalog.phrases(v.name), formation_names))
        .collect()
}

fn project_verb(verb: &PlatformVerb, phrases: &[String], formation_names: &[String]) -> IntentDoc {
    // The verb's own words are the strongest signal; the sentence
    // templates carry the synonyms ("pause", "hold", "stop for now").
    let mut name_stems: Vec<String> = tokenize(&verb.name.replace(['.', '_'], " "))
        .into_iter()
        .map(|t| t.stem)
        .collect();
    for phrase in phrases {
        // Slot markers are not words the user says.
        let bare = strip_slots(phrase);
        name_stems.extend(tokenize(&bare).into_iter().map(|t| t.stem));
    }
    name_stems.sort();
    name_stems.dedup();

    let desc_stems = tokenize(verb.description)
        .into_iter()
        .map(|t| t.stem)
        .collect();

    let mut slots = Vec::new();
    if verb.takes_formation() {
        slots.push(formation_slot(formation_names));
    }

    IntentDoc {
        recipe_id: format!("{PLATFORM_PREFIX}{}", verb.name),
        name: verb.name.to_owned(),
        description: verb.description.to_owned(),
        // Not a recipe category in any real sense — platform documents are
        // routed by `platform_verb`, never by category.
        category: RecipeCategory::Custom,
        ai_required: false,
        name_stems,
        tag_stems: vec![verb.group.as_str().to_owned()],
        desc_stems,
        slots,
        platform_verb: Some(verb.name),
    }
}

/// `pause {formation}` → `pause`.
fn strip_slots(phrase: &str) -> String {
    let mut out = String::with_capacity(phrase.len());
    let mut depth = 0usize;
    for ch in phrase.chars() {
        match ch {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            c if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

/// The `{formation}` slot — a `Select` over the live formation names, so
/// the existing gazetteer extractor fills it from the sentence.
fn formation_slot(formation_names: &[String]) -> SlotSpec {
    let options: Vec<SelectOption> = formation_names
        .iter()
        .map(|n| SelectOption {
            value: n.clone(),
            label: n.clone(),
        })
        .collect();
    let gazetteer = Gazetteer::from_options(
        options
            .iter()
            .map(|o| (o.value.clone(), o.label.clone()))
            .collect::<Vec<_>>(),
    );
    SlotSpec {
        field: InputField {
            id: "formation".to_owned(),
            label: "formation".to_owned(),
            kind: FieldKind::Select { options },
            visibility: FieldVisibility::Required,
            default: None,
            hint: None,
        },
        tag: SlotKindTag::Select,
        gazetteer: Some(gazetteer),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn names() -> Vec<String> {
        vec!["Research Squad".to_owned(), "Watchtower".to_owned()]
    }

    #[test]
    fn test_one_document_per_platform_verb() {
        let docs = platform_docs("en", &names());
        assert_eq!(docs.len(), platform_verbs().len());
        assert!(docs.iter().all(|d| d.platform_verb.is_some()));
    }

    #[test]
    fn test_formation_slot_reads_the_live_names() {
        let docs = platform_docs("en", &names());
        let pause = docs
            .iter()
            .find(|d| d.platform_verb == Some("formation.pause"))
            .expect("pause doc");
        let slot = pause.slot("formation").expect("formation slot");
        let g = slot.gazetteer.as_ref().expect("gazetteer");
        assert!(g.match_in("hold the research squad please").is_some());
        // A name that is not in the live roster does not match.
        assert!(g.match_in("hold the kitchen brigade").is_none());
    }

    #[test]
    fn test_synonyms_from_the_sentence_file_reach_the_token_bag() {
        let docs = platform_docs("en", &names());
        let pause = docs
            .iter()
            .find(|d| d.platform_verb == Some("formation.pause"))
            .expect("pause doc");
        assert!(pause.name_stems.iter().any(|s| s.starts_with("hold")));
    }

    /// The drum rule again, this time over what chat can *understand*.
    #[test]
    fn test_no_platform_document_assigns_work_to_a_member() {
        for doc in platform_docs("en", &names()) {
            assert!(!doc.name.contains("assign"), "{} assigns", doc.name);
            assert!(
                !doc.slots
                    .iter()
                    .any(|s| s.id() == "member" || s.id() == "agent"),
                "{} takes a member slot",
                doc.name
            );
        }
    }
}
