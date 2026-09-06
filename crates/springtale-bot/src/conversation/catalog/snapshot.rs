//! Catalogue snapshot — the recipe library projected into the shape
//! the NLU engine scores and slot-fills against.
//!
//! Built fresh each turn from `list_recipes` (≈60 built-ins; cheap).
//! Every recipe becomes an [`IntentDoc`]: stemmed name/tag/description
//! token bags for intent scoring, plus a [`SlotSpec`] per input field
//! carrying a precomputed [`Gazetteer`] for `Select` fields so the
//! same pass that names the recipe can pre-fill its slots.

use springtale_runtime::operations::recipes::types::{
    DerivedInputResolver, FieldKind, FieldVisibility, InputField, Recipe, RecipeCategory,
};

use crate::conversation::nlu::gazetteer::Gazetteer;
use crate::conversation::nlu::normalize::{stem, tokenize};

/// Coarse classification of a field kind — lets the dialogue pick an
/// extractor and a prompt style without re-matching the full
/// `FieldKind` everywhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotKindTag {
    Text,
    Secret,
    Number,
    Bool,
    Url,
    Select,
    Cron,
    /// A free-text `Text` input that the recipe geocodes at deploy time
    /// (it's the `source_input_id` of a `Geocode` derived input). The
    /// engine extracts a place phrase from the sentence for it. The one
    /// declaration (`derived_inputs`) drives both chat extraction and
    /// deploy-time resolution.
    Place,
    Other,
}

/// One input field, ready for extraction + prompting.
#[derive(Debug, Clone)]
pub struct SlotSpec {
    pub field: InputField,
    pub tag: SlotKindTag,
    /// Precomputed gazetteer for `Select` fields (value↔label), else None.
    pub gazetteer: Option<Gazetteer>,
}

impl SlotSpec {
    pub fn id(&self) -> &str {
        &self.field.id
    }
    pub fn label(&self) -> &str {
        &self.field.label
    }
    pub fn is_required(&self) -> bool {
        self.field.visibility == FieldVisibility::Required
    }
    /// Baked fields are never surfaced to the user.
    pub fn is_user_facing(&self) -> bool {
        self.field.visibility != FieldVisibility::Baked
    }
    pub fn default(&self) -> Option<&serde_json::Value> {
        self.field.default.as_ref()
    }
}

/// A recipe projected for matching.
#[derive(Debug, Clone)]
pub struct IntentDoc {
    pub recipe_id: String,
    pub name: String,
    pub description: String,
    pub category: RecipeCategory,
    pub ai_required: bool,
    /// Stemmed tokens from the recipe name (highest-weight signal).
    pub name_stems: Vec<String>,
    /// Stemmed tokens from tags + connector names.
    pub tag_stems: Vec<String>,
    /// Stemmed tokens from the description.
    pub desc_stems: Vec<String>,
    pub slots: Vec<SlotSpec>,
    /// Set on the documents built from `platform::platform_docs` — the
    /// dotted platform verb this document stands for (plan 5.4). `None`
    /// for recipes. Routing reads this and nothing else: a document with
    /// a verb runs a command, a document without one starts a setup
    /// frame.
    pub platform_verb: Option<&'static str>,
}

impl IntentDoc {
    pub fn slot(&self, id: &str) -> Option<&SlotSpec> {
        self.slots.iter().find(|s| s.id() == id)
    }

    /// Labels of the credentials this recipe requires the user to supply.
    /// Credentials are NEVER collected in chat (a plaintext token must not
    /// land in the session store) — the engine hands these recipes off to
    /// the secure Library / connector flow where secrets go to the vault.
    pub fn required_secret_labels(&self) -> Vec<String> {
        self.slots
            .iter()
            .filter(|s| s.tag == SlotKindTag::Secret && s.is_required() && s.default().is_none())
            .map(|s| s.label().to_owned())
            .collect()
    }

    /// True when this recipe needs a credential the user would have to type.
    pub fn requires_secret(&self) -> bool {
        !self.required_secret_labels().is_empty()
    }

    /// Labels of Required inputs the chat engine CAN'T collect — secrets
    /// (never in chat) and `Other` kinds (CssSelector/JsonSchema/
    /// WorkspaceTarget need a picker/editor). A recipe with any of these
    /// is handed off to the Library so it never dead-ends in a re-ask
    /// loop. `Place`/`Select`/`Text`/etc. are collectable and excluded.
    pub fn handoff_labels(&self) -> Vec<String> {
        self.slots
            .iter()
            .filter(|s| {
                s.is_required()
                    && s.default().is_none()
                    && matches!(s.tag, SlotKindTag::Secret | SlotKindTag::Other)
            })
            .map(|s| s.label().to_owned())
            .collect()
    }

    /// True when the recipe needs an input the chat engine can't collect.
    pub fn requires_handoff(&self) -> bool {
        !self.handoff_labels().is_empty()
    }
}

/// The whole catalogue, projected.
#[derive(Debug, Clone, Default)]
pub struct CatalogSnapshot {
    pub intents: Vec<IntentDoc>,
}

impl CatalogSnapshot {
    pub fn build(recipes: Vec<Recipe>) -> Self {
        let intents = recipes.into_iter().map(project_recipe).collect();
        Self { intents }
    }

    /// The catalogue plus one document per platform verb (plan 5.4).
    /// `formation_names` is the live roster, read at match time so the
    /// `{formation}` slot list is never hard-coded.
    pub fn build_with_platform(
        recipes: Vec<Recipe>,
        locale: &str,
        formation_names: &[String],
    ) -> Self {
        let mut intents: Vec<IntentDoc> = recipes.into_iter().map(project_recipe).collect();
        intents.extend(super::platform::platform_docs(locale, formation_names));
        Self { intents }
    }

    /// The document for a dotted platform verb, if it is in the snapshot.
    pub fn find_verb(&self, verb: &str) -> Option<&IntentDoc> {
        self.intents.iter().find(|d| d.platform_verb == Some(verb))
    }

    pub fn find(&self, recipe_id: &str) -> Option<&IntentDoc> {
        self.intents.iter().find(|d| d.recipe_id == recipe_id)
    }
}

fn project_recipe(r: Recipe) -> IntentDoc {
    let name_stems = tokenize(&r.name).into_iter().map(|t| t.stem).collect();

    let mut tag_stems: Vec<String> = Vec::new();
    for tag in &r.tags {
        tag_stems.extend(tokenize(tag).into_iter().map(|t| t.stem));
    }
    for c in &r.connectors_used {
        // "connector-telegram" → "telegram"
        let bare = c.strip_prefix("connector-").unwrap_or(c);
        tag_stems.push(stem(&bare.to_lowercase()));
    }

    let desc_stems = tokenize(&r.description)
        .into_iter()
        .map(|t| t.stem)
        .collect();

    // Inputs that are geocoded at deploy time are "places" — the engine
    // extracts a free-text place phrase for them from the sentence.
    let place_ids: std::collections::HashSet<&str> = r
        .blueprint
        .derived_inputs
        .iter()
        .map(|d| match d {
            DerivedInputResolver::Geocode {
                source_input_id, ..
            } => source_input_id.as_str(),
        })
        .collect();

    let slots = r
        .inputs
        .iter()
        .map(|f| project_field(f, place_ids.contains(f.id.as_str())))
        .collect();

    IntentDoc {
        recipe_id: r.id,
        name: r.name,
        description: r.description,
        category: r.category,
        ai_required: r.ai_required,
        name_stems,
        tag_stems,
        desc_stems,
        slots,
        platform_verb: None,
    }
}

fn project_field(f: &InputField, is_place: bool) -> SlotSpec {
    let (tag, gazetteer) = match &f.kind {
        // A Text input the recipe geocodes is a Place (extractable target).
        FieldKind::Text if is_place => (SlotKindTag::Place, None),
        FieldKind::Text => (SlotKindTag::Text, None),
        FieldKind::Secret => (SlotKindTag::Secret, None),
        FieldKind::Number => (SlotKindTag::Number, None),
        FieldKind::Bool => (SlotKindTag::Bool, None),
        FieldKind::Url => (SlotKindTag::Url, None),
        FieldKind::Cron => (SlotKindTag::Cron, None),
        FieldKind::Select { options } => {
            let g =
                Gazetteer::from_options(options.iter().map(|o| (o.value.clone(), o.label.clone())));
            (SlotKindTag::Select, Some(g))
        }
        _ => (SlotKindTag::Other, None),
    };
    SlotSpec {
        field: f.clone(),
        tag,
        gazetteer,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use springtale_runtime::operations::recipes::types::{
        Difficulty, RecipeBlueprint, RecipeSource, SelectOption,
    };

    fn weather() -> Recipe {
        Recipe {
            id: "weather-briefing".into(),
            name: "Morning Weather".into(),
            description: "Today's weather every morning.".into(),
            icon_id: "cloud".into(),
            category: RecipeCategory::Daily,
            tags: vec!["cron".into(), "weather".into()],
            connectors_used: vec!["connector-http".into()],
            ai_required: false,
            difficulty: Difficulty::Quick,
            source: RecipeSource::Builtin,
            inputs: vec![InputField {
                id: "location".into(),
                label: "City".into(),
                kind: FieldKind::Select {
                    options: vec![SelectOption {
                        value: "lat=tus".into(),
                        label: "Tucson".into(),
                    }],
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

    #[test]
    fn test_projects_name_and_tags() {
        let snap = CatalogSnapshot::build(vec![weather()]);
        let doc = snap.find("weather-briefing").unwrap();
        assert!(doc.name_stems.contains(&"weather".to_owned()));
        assert!(doc.tag_stems.contains(&"http".to_owned())); // connector bare name
        assert_eq!(doc.slots.len(), 1);
        assert_eq!(doc.slots[0].tag, SlotKindTag::Select);
        assert!(doc.slots[0].gazetteer.is_some());
    }

    #[test]
    fn test_slot_required_and_gazetteer_built() {
        let snap = CatalogSnapshot::build(vec![weather()]);
        let doc = snap.find("weather-briefing").unwrap();
        let slot = doc.slot("location").unwrap();
        assert!(slot.is_required());
        let hit = slot
            .gazetteer
            .as_ref()
            .unwrap()
            .match_in("weather in tucson");
        assert_eq!(hit.unwrap().value, "lat=tus");
    }
}
