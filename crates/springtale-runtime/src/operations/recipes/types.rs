//! Recipe types — the click-and-play blueprint a frontend renders.
//!
//! A [`Recipe`] is a structured starting point that a survivor picks
//! from the [`crate::operations::recipes::library`] surface. Choosing
//! a recipe pulls up the `required_inputs` (must-fill) and surfaces
//! `optional_inputs` / `advanced_inputs` behind progressive disclosure
//! (W1.C). Submitting the form runs
//! [`crate::operations::recipes::apply::apply_recipe`], which composes
//! existing runtime ops to materialise a working bot.
//!
//! ## Architecture — backend owns the decisions
//!
//! Every "is this field required vs optional vs baked-in" decision is
//! encoded in the `Recipe` value the backend returns. The frontend
//! renders what it's told and never invents categories or classifies
//! fields on its own (per `feedback_thin_frontend_modular_backend` /
//! `feedback_zero_frontend_logic`). The same `Recipe` shape feeds
//! desktop IPC, dashboard HTTP, and future frontends.
//!
//! ## Trust + source
//!
//! Recipes carry a [`RecipeSource`] so the UI can render trust badges
//! (W3.A). `Builtin` recipes ship in the binary; `User` recipes live
//! in the SQLite `recipes_user` table; `Community` is wire-shaped now
//! and lit up when the marketplace lands.

use serde::{Deserialize, Serialize};
use specta::Type;

/// Top-level catalogued recipe.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct Recipe {
    /// Stable identifier (kebab-case). Built-ins use literal slugs
    /// (`"telegram-echo"`); user recipes use UUIDs.
    pub id: String,
    pub name: String,
    pub description: String,
    /// Sprite id rendered by the frontend (`@springtale/ui` sprite map).
    pub icon_id: String,
    pub category: RecipeCategory,
    /// Free-form filter tags (`["telegram", "ai-optional"]`).
    pub tags: Vec<String>,
    pub connectors_used: Vec<String>,
    /// `true` if the recipe needs a working AI adapter to operate
    /// meaningfully. Surfaces in the card so users running NoopAdapter
    /// can self-filter.
    pub ai_required: bool,
    pub difficulty: Difficulty,
    pub source: RecipeSource,
    /// Author-declared input fields. Each carries its own
    /// `FieldVisibility` (Required / Optional / Advanced / Baked) so
    /// the progressive-disclosure UI renders the right tier and
    /// preflight blocks deploy on missing Required values. Order is
    /// the author's preferred render order — frontend does not
    /// re-sort by visibility.
    pub inputs: Vec<InputField>,
    /// What `apply_recipe` actually does once the inputs are filled.
    pub blueprint: RecipeBlueprint,
}

impl Recipe {
    /// Iterate inputs whose [`FieldVisibility`] matches the predicate.
    /// Preserves author-declared order. Used by deploy form, preview,
    /// and preflight to render / validate one tier at a time.
    pub fn inputs_with(
        &self,
        visibility: FieldVisibility,
    ) -> impl Iterator<Item = &InputField> {
        self.inputs
            .iter()
            .filter(move |f| f.visibility == visibility)
    }

    /// Inputs the user must fill before deploy.
    pub fn required_inputs(&self) -> impl Iterator<Item = &InputField> {
        self.inputs_with(FieldVisibility::Required)
    }

    /// Inputs the user may tweak under "Show more options."
    pub fn optional_inputs(&self) -> impl Iterator<Item = &InputField> {
        self.inputs_with(FieldVisibility::Optional)
    }

    /// Power-user inputs revealed under "Show advanced."
    pub fn advanced_inputs(&self) -> impl Iterator<Item = &InputField> {
        self.inputs_with(FieldVisibility::Advanced)
    }

    /// Baked inputs the user never sees; the recipe ships their
    /// default value unchanged.
    pub fn baked_inputs(&self) -> impl Iterator<Item = &InputField> {
        self.inputs_with(FieldVisibility::Baked)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Type)]
#[serde(rename_all = "snake_case")]
pub enum RecipeCategory {
    Messaging,
    Coding,
    Web,
    AiAssistant,
    Daily,
    SafetyPrivacy,
    Custom,
}

impl RecipeCategory {
    pub fn label(self) -> &'static str {
        match self {
            Self::Messaging => "Messaging",
            Self::Coding => "Coding",
            Self::Web => "Web",
            Self::AiAssistant => "AI assistants",
            Self::Daily => "Daily tasks",
            Self::SafetyPrivacy => "Safety & Privacy",
            Self::Custom => "Custom",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum Difficulty {
    /// 3-click deploy.
    Quick,
    /// 5-ish clicks, a few optional knobs.
    Standard,
    /// Power user — multi-agent, intricate trigger graph, etc.
    Power,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RecipeSource {
    /// Compiled into the binary.
    Builtin,
    /// Saved by the local user via W2.B authoring flow.
    User,
    /// Reserved for the future community marketplace; carries author
    /// identity + Ed25519 signature so the sentinel can verify before
    /// install (W3.A; wire-shape only today).
    Community {
        author: String,
        signature: String,
    },
}

/// A typed input field the user fills (or that has a default the
/// recipe author baked in).
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct InputField {
    pub id: String,
    pub label: String,
    pub kind: FieldKind,
    /// Author-declared visibility. Drives progressive-disclosure
    /// layering (Required → Optional → Advanced) and the
    /// recipe-authoring flow (`Baked` fields never show as user
    /// inputs — they ship the recipe's default unchanged).
    ///
    /// Per industry research (IFTTT applets, Zapier Zap templates,
    /// GitHub Actions input metadata), this field is **author-declared
    /// per field**, never inferred. No `auto_classify` heuristic.
    pub visibility: FieldVisibility,
    /// Default value rendered as the field's initial value. `None`
    /// means the field is empty until the user types.
    #[serde(default)]
    pub default: Option<serde_json::Value>,
    /// Short hint rendered under the input (e.g. "Get this from
    /// @BotFather on Telegram"). Plaintext only — never user-supplied
    /// HTML.
    #[serde(default)]
    pub hint: Option<String>,
}

/// Where an [`InputField`] surfaces in the progressive-disclosure
/// deploy form, and whether the user is asked at all.
///
/// Authors declare this explicitly per field. The frontend renders
/// each tier behind a chevron (Apple print-dialog "Show Details"
/// pattern) so a click-and-play user can deploy after touching only
/// `Required` fields, while a power user can drill into `Optional`
/// → `Advanced` on the same surface.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum FieldVisibility {
    /// User MUST fill this before deploy. The W1.D preflight blocks
    /// the Deploy button while any Required is empty.
    Required,
    /// User MAY tweak; revealed under "Show more options."
    Optional,
    /// Power-user surface; revealed under "Show advanced."
    Advanced,
    /// Author baked a value — the user never sees this field, even
    /// under "Show advanced." The recipe ships with the default
    /// unchanged. Use this for placeholders the author wants
    /// computed (e.g. a derived field) or for fields the recipe's
    /// summary explicitly documents.
    Baked,
}

/// Internally-tagged so the JSON wire matches the TS discriminated-union
/// shape `{ kind: "text" } | { kind: "select", options: [...] }`. Unit
/// variants serialize as `{ "kind": "text" }`, struct variants flatten
/// the fields next to the tag. Without this serde defaults to external
/// tagging, which puts unit variants on the wire as bare strings and
/// breaks the frontend type contract for input rendering.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FieldKind {
    /// Free-form text (no secrecy).
    Text,
    /// Sensitive value — frontend renders as password input + stores
    /// through the vault rather than plain config.
    Secret,
    /// Numeric input.
    Number,
    /// Boolean toggle.
    Bool,
    /// URL (validated to scheme http/https).
    Url,
    /// Picked from a fixed list of options.
    Select { options: Vec<SelectOption> },
    /// Cron expression (5- or 6-field). Frontend renders a
    /// `CronFrequencyChip` next to the input that shows:
    ///   - red 🔴 on sub-minute schedules (preflight blocking)
    ///   - yellow 🟡 on 1–4 minute polls (preflight warning)
    ///   - green 🟢 on ≥5 minute cadences (preflight verified)
    /// Plus a "next 5 fire times" preview computed against the
    /// `cron` crate. Surfaces the same classification the backend's
    /// `check_schedule_frequency` produces so frontend never invents
    /// its own thresholds (per `feedback_zero_frontend_logic`).
    Cron,
    /// CSS selector. Frontend opens the Tauri webview selector picker
    /// at the recipe's target URL — hover-highlights elements,
    /// generates `:nth-of-type` selectors, posts back. Phase B
    /// activates the picker; for now the field renders as text input
    /// with a "test against URL" probe button.
    CssSelector {
        /// Recipe input id whose value the selector picker should
        /// load when launched. E.g. `"watch_url"`. Optional — when
        /// `None`, the picker prompts for the URL inline.
        #[serde(default)]
        sample_url: Option<String>,
    },
    /// JSON Schema for the AI-driven extraction step. Frontend
    /// renders a Monaco-style schema editor (Phase B); falls back to
    /// a textarea today. Schema posts as `serde_json::Value` since
    /// it carries arbitrary user-defined fields.
    JsonSchema {
        /// Optional example payload the editor shows alongside the
        /// schema. Recipe authors use this to convey "extract this
        /// shape from a page that looks like X."
        #[serde(default)]
        example: Option<serde_json::Value>,
    },
    /// D1 — external workspace target (messaging chat / channel /
    /// group / user). Renders as a dropdown over the formation's
    /// `mental_model_workspaces` filtered by `connector`, plus a
    /// 🔍 Scan button (active discovery via the connector's
    /// `discover_destinations` action), a 🎯 Onboard button
    /// (Telegram-specific deep-link affordance), and an
    /// ✏️ Manual entry escape hatch. At deploy time the field
    /// resolves to the raw destination id parsed off the chosen
    /// `WorkspaceKey` URI — recipe TOML stays string-for-string
    /// compatible with the existing `${chat_id}` / `${to}`
    /// substitution. See `docs/intended-arch/COOPERATION.md §21`
    /// (Shared Mental Model — Directory Facilitator extension).
    WorkspaceTarget {
        /// Connector this destination belongs to. e.g.
        /// `"connector-telegram"`.
        connector: String,
        /// Optional `kind` filter — `["channel"]` excludes user DMs,
        /// `["user", "group"]` excludes channels. Empty / unset =
        /// no filter (show everything for this connector).
        #[serde(default)]
        kinds: Vec<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Type)]
pub struct SelectOption {
    pub value: String,
    pub label: String,
}

/// Blueprint — what `apply_recipe` performs against the running
/// runtime. All strings may contain `${input_id}` placeholders which
/// [`crate::operations::recipes::apply`] substitutes from user inputs.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct RecipeBlueprint {
    /// Connector configs to upsert (keyed by connector name).
    #[serde(default)]
    pub connector_configs: Vec<ConnectorConfigStep>,
    /// Rules to create.
    #[serde(default)]
    pub rules: Vec<RuleStep>,
    /// AI adapter config to apply, scoped to `ai:global`, an agent,
    /// or a formation.
    #[serde(default)]
    pub ai_config: Option<AiConfigStep>,
    /// Plain-language "what this will do" summary rendered in the
    /// quick-view. Backend-authored so it stays accurate as recipes
    /// evolve (vs. relying on the frontend to compose a summary).
    #[serde(default)]
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ConnectorConfigStep {
    pub connector_name: String,
    /// JSON value template — placeholders substituted before passing
    /// to [`crate::operations::config::upsert_connector_config`].
    pub config: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct RuleStep {
    /// TOML rule body — placeholders substituted before parse.
    pub toml: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct AiConfigStep {
    /// Target key (`"ai:global"`, `"ai:{agent_id}"`, `"ai:formation:{id}"`).
    pub target: String,
    pub config: serde_json::Value,
}

/// Filter passed to [`crate::operations::recipes::library::list_recipes`].
///
/// All filtering happens server-side. Frontend sends the filter, gets
/// the matching slice back — never holds the full catalogue and never
/// filters in-memory.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Type)]
pub struct RecipeFilter {
    /// Fuzzy match on `name` + `description` + connectors_used.
    #[serde(default)]
    pub query: Option<String>,
    /// Restrict to a single category. `None` = all.
    #[serde(default)]
    pub category: Option<RecipeCategory>,
    /// Free-form tag filter (AND across multiple).
    #[serde(default)]
    pub tags: Vec<String>,
    /// Source filter: `["builtin"]`, `["user"]`, `["community"]`, or any
    /// combination. Empty = all sources.
    #[serde(default)]
    pub sources: Vec<RecipeSourceFilter>,
    /// Whether to include only favorites.
    #[serde(default)]
    pub favorites_only: bool,
    /// Maximum results to return (server caps at 100 even if larger).
    #[serde(default)]
    pub limit: Option<usize>,
    /// Sort order.
    #[serde(default)]
    pub sort: RecipeSort,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum RecipeSourceFilter {
    Builtin,
    User,
    Community,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum RecipeSort {
    /// Recommended order — built-ins first, sorted by Difficulty (Quick first).
    #[default]
    Recommended,
    /// Alphabetical by name.
    Name,
    /// Most recently used (user-recent then everything else).
    Recent,
}

/// What the user submitted when picking a recipe.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct RecipeInputs {
    /// Map from `InputField.id` → user-supplied value.
    pub values: std::collections::BTreeMap<String, serde_json::Value>,
}

impl RecipeInputs {
    pub fn empty() -> Self {
        Self {
            values: std::collections::BTreeMap::new(),
        }
    }
    pub fn get(&self, id: &str) -> Option<&serde_json::Value> {
        self.values.get(id)
    }
    pub fn insert(&mut self, id: impl Into<String>, value: serde_json::Value) {
        self.values.insert(id.into(), value);
    }
}

/// Outcome of a successful `apply_recipe` call.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ApplyReport {
    /// Recipe that was applied.
    pub recipe_id: String,
    /// Connector names whose config was upserted.
    pub connectors_configured: Vec<String>,
    /// Rule ids created.
    pub rules_created: Vec<String>,
    /// Whether the AI adapter step ran.
    pub ai_configured: bool,
    /// Plain-language summary rendered to the user post-deploy.
    pub summary: String,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn recipe_round_trips_through_json() {
        let recipe = Recipe {
            id: "telegram-echo".into(),
            name: "Telegram Echo".into(),
            description: "Reply with the same message".into(),
            icon_id: "telegram".into(),
            category: RecipeCategory::Messaging,
            tags: vec!["telegram".into()],
            connectors_used: vec!["connector-telegram".into()],
            ai_required: false,
            difficulty: Difficulty::Quick,
            source: RecipeSource::Builtin,
            inputs: vec![InputField {
                id: "bot_token".into(),
                label: "Bot token".into(),
                kind: FieldKind::Secret,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some("Get this from @BotFather".into()),
            }],
            blueprint: RecipeBlueprint {
                connector_configs: vec![ConnectorConfigStep {
                    connector_name: "connector-telegram".into(),
                    config: serde_json::json!({ "bot_token": "${bot_token}" }),
                }],
                rules: vec![],
                ai_config: None,
                summary: Some("Telegram echo bot".into()),
            },
        };
        let json = serde_json::to_string(&recipe).unwrap();
        let back: Recipe = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "telegram-echo");
        assert_eq!(back.inputs.len(), 1);
        assert!(matches!(back.inputs[0].kind, FieldKind::Secret));
        assert_eq!(back.inputs[0].visibility, FieldVisibility::Required);
    }

    #[test]
    fn select_kind_round_trips() {
        let kind = FieldKind::Select {
            options: vec![
                SelectOption { value: "ollama".into(), label: "Ollama (local)".into() },
                SelectOption { value: "openai".into(), label: "OpenAI".into() },
            ],
        };
        let json = serde_json::to_string(&kind).unwrap();
        let back: FieldKind = serde_json::from_str(&json).unwrap();
        match back {
            FieldKind::Select { options } => assert_eq!(options.len(), 2),
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn default_filter_is_empty() {
        let f = RecipeFilter::default();
        assert!(f.query.is_none());
        assert!(f.category.is_none());
        assert!(f.tags.is_empty());
        assert!(matches!(f.sort, RecipeSort::Recommended));
    }
}
