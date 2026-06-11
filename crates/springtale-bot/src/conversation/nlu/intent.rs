//! Deterministic intent ranking.
//!
//! Scores a free-text utterance against every recipe in the
//! [`CatalogSnapshot`] using weighted token overlap (name > tags >
//! description), synonym/concept expansion, gazetteer slot hits, and a
//! Jaro–Winkler typo bonus. No ML — the score is a pure function of the
//! utterance and the catalogue, so the same message always ranks the
//! same way (Snips' "deterministic parser" guarantee).
//!
//! The caller turns the ranked list into a decision: a confident single
//! winner starts a setup frame, a tight cluster asks the user to
//! choose, and an empty field becomes a capability answer.

use crate::conversation::catalog::{CatalogSnapshot, IntentDoc};

use super::fuzzy::jaro_winkler;
use super::normalize::tokenize;
use super::synonyms;

/// Weights for where a token matched. Name is the strongest signal.
const W_NAME: f32 = 3.0;
const W_TAG: f32 = 2.0;
const W_DESC: f32 = 1.0;
/// A gazetteer slot hit (an option label appearing in the utterance) is
/// strong evidence for the owning recipe.
const W_SLOT_HIT: f32 = 2.5;
/// Fuzzy matches contribute their similarity scaled by this so a typo
/// never outranks an exact hit.
const W_FUZZY: f32 = 0.8;
/// Below this Jaro–Winkler similarity a fuzzy match is ignored.
const FUZZY_FLOOR: f64 = 0.90;

/// Minimum score for a recipe to be considered a candidate at all.
pub const INTENT_FLOOR: f32 = 2.0;
/// A winner must beat the runner-up by at least this much to be taken
/// as confident; otherwise the matches are "ambiguous" and we clarify.
pub const CONFIDENCE_MARGIN: f32 = 1.5;

/// One scored recipe.
#[derive(Debug, Clone)]
pub struct IntentCandidate {
    pub recipe_id: String,
    pub name: String,
    pub score: f32,
    /// Slot ids that the utterance already pre-filled via gazetteer hits.
    pub slot_hits: Vec<String>,
}

/// The outcome of ranking — what the dialogue layer should do next.
#[derive(Debug, Clone)]
pub enum IntentDecision {
    /// One clear winner — start its setup frame.
    Confident(IntentCandidate),
    /// A tight cluster — ask the user to pick.
    Ambiguous(Vec<IntentCandidate>),
    /// Nothing matched — answer with capabilities / hand off to AI.
    NoMatch,
}

/// Rank every recipe; return candidates sorted best-first (score ≥ floor).
pub fn rank(utterance: &str, catalog: &CatalogSnapshot) -> Vec<IntentCandidate> {
    // Build the scoring bag: utterance stems plus their concept expansions.
    let toks = tokenize(utterance);
    let mut bag: Vec<String> = Vec::new();
    for t in &toks {
        bag.push(t.stem.clone());
        for ex in synonyms::expand(&t.stem) {
            bag.push((*ex).to_owned());
        }
    }

    let mut out: Vec<IntentCandidate> = catalog
        .intents
        .iter()
        .map(|doc| score_doc(utterance, &bag, doc))
        .filter(|c| c.score >= INTENT_FLOOR)
        .collect();

    out.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.name.cmp(&b.name))
    });
    out
}

/// Turn the ranked list into a decision.
pub fn decide(mut ranked: Vec<IntentCandidate>) -> IntentDecision {
    match ranked.len() {
        0 => IntentDecision::NoMatch,
        1 => IntentDecision::Confident(ranked.remove(0)),
        _ => {
            let gap = ranked[0].score - ranked[1].score;
            if gap >= CONFIDENCE_MARGIN {
                IntentDecision::Confident(ranked.remove(0))
            } else {
                // Keep the tight cluster at the top (within margin of #1).
                let top = ranked[0].score;
                ranked.retain(|c| top - c.score < CONFIDENCE_MARGIN);
                ranked.truncate(3);
                IntentDecision::Ambiguous(ranked)
            }
        }
    }
}

fn score_doc(utterance: &str, bag: &[String], doc: &IntentDoc) -> IntentCandidate {
    let mut score = 0.0f32;

    score += overlap(bag, &doc.name_stems, W_NAME);
    score += overlap(bag, &doc.tag_stems, W_TAG);
    score += overlap(bag, &doc.desc_stems, W_DESC);

    // Fuzzy: reward near-misses against name/tag tokens for typo tolerance,
    // but only for bag tokens that didn't already match exactly.
    score += fuzzy_bonus(bag, &doc.name_stems, W_NAME);
    score += fuzzy_bonus(bag, &doc.tag_stems, W_TAG);

    // Gazetteer slot hits: an option label present in the utterance both
    // boosts the recipe and records which slot it pre-filled.
    let mut slot_hits = Vec::new();
    for slot in &doc.slots {
        if let Some(g) = &slot.gazetteer
            && g.match_in(utterance).is_some()
        {
            score += W_SLOT_HIT;
            slot_hits.push(slot.id().to_owned());
        }
    }

    IntentCandidate {
        recipe_id: doc.recipe_id.clone(),
        name: doc.name.clone(),
        score,
        slot_hits,
    }
}

/// Sum of `weight` for each distinct bag token that appears in `field`.
fn overlap(bag: &[String], field: &[String], weight: f32) -> f32 {
    let mut hits = 0.0;
    for token in dedup(bag) {
        if field.iter().any(|f| f == &token) {
            hits += weight;
        }
    }
    hits
}

fn fuzzy_bonus(bag: &[String], field: &[String], weight: f32) -> f32 {
    let mut best_per_token = 0.0f32;
    for token in dedup(bag) {
        // Skip tokens that already match exactly — no double counting.
        if field.iter().any(|f| f == &token) {
            continue;
        }
        if token.len() < 4 {
            continue;
        }
        let mut best = 0.0f64;
        for f in field {
            if f.len() < 4 {
                continue;
            }
            let sim = jaro_winkler(&token, f);
            if sim > best {
                best = sim;
            }
        }
        if best >= FUZZY_FLOOR {
            best_per_token += weight * W_FUZZY * (best as f32);
        }
    }
    best_per_token
}

fn dedup(bag: &[String]) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for t in bag {
        if !seen.contains(t) {
            seen.push(t.clone());
        }
    }
    seen
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::conversation::catalog::CatalogSnapshot;
    use springtale_runtime::operations::recipes::types::{
        Difficulty, FieldKind, FieldVisibility, InputField, Recipe, RecipeBlueprint,
        RecipeCategory, RecipeSource, SelectOption,
    };

    fn recipe(id: &str, name: &str, desc: &str, tags: &[&str], loc_opts: &[&str]) -> Recipe {
        Recipe {
            id: id.into(),
            name: name.into(),
            description: desc.into(),
            icon_id: "x".into(),
            category: RecipeCategory::Daily,
            tags: tags.iter().map(|s| (*s).to_owned()).collect(),
            connectors_used: vec![],
            ai_required: false,
            difficulty: Difficulty::Quick,
            source: RecipeSource::Builtin,
            inputs: vec![InputField {
                id: "location".into(),
                label: "City".into(),
                kind: FieldKind::Select {
                    options: loc_opts
                        .iter()
                        .map(|l| SelectOption {
                            value: format!("v-{l}"),
                            label: (*l).to_owned(),
                        })
                        .collect(),
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
        }
    }

    fn catalog() -> CatalogSnapshot {
        CatalogSnapshot::build(vec![
            recipe(
                "weather-briefing",
                "Morning Weather",
                "Today's weather every morning",
                &["weather", "cron"],
                &["Tucson", "Phoenix"],
            ),
            recipe(
                "telegram-echo",
                "Telegram Echo",
                "Reply with the same message",
                &["telegram", "echo"],
                &[],
            ),
        ])
    }

    #[test]
    fn test_direct_name_match_is_confident() {
        let ranked = rank("set up the morning weather", &catalog());
        assert_eq!(ranked[0].recipe_id, "weather-briefing");
        assert!(matches!(decide(ranked), IntentDecision::Confident(_)));
    }

    #[test]
    fn test_synonym_match() {
        // "forecast" is not in any recipe, but expands to "weather".
        let ranked = rank("give me the forecast", &catalog());
        assert_eq!(ranked[0].recipe_id, "weather-briefing");
    }

    #[test]
    fn test_typo_still_matches() {
        let ranked = rank("set up the wether briefing", &catalog());
        assert!(!ranked.is_empty());
        assert_eq!(ranked[0].recipe_id, "weather-briefing");
    }

    #[test]
    fn test_gazetteer_hit_prefills_slot() {
        let ranked = rank("weather in Tucson", &catalog());
        assert_eq!(ranked[0].recipe_id, "weather-briefing");
        assert!(ranked[0].slot_hits.contains(&"location".to_owned()));
    }

    #[test]
    fn test_no_match_is_empty() {
        let ranked = rank("what is the meaning of life", &catalog());
        assert!(matches!(decide(ranked), IntentDecision::NoMatch));
    }
}
