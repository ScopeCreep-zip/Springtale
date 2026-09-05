//! Individual preflight checks.
//!
//! Each check returns a single [`PreflightItem`]. The W1.D engine
//! gathers them concurrently. Checks must be cheap or capped by an
//! internal timeout — preflight runs on every field-change (debounced
//! 500ms by the frontend) so a slow check would visibly stall the
//! form. Network probes that need real round-trip latency (AI
//! provider HEAD, webhook reachability) are explicitly opt-in and
//! tagged so the engine can run them out-of-band.

use serde_json::Value;

use crate::state::RuntimeState;

use super::super::recipes::types::{FieldKind, InputField, Recipe, RecipeInputs};
use super::types::{PreflightFix, PreflightItem, PreflightStatus};

/// Check that every required input has a non-empty value.
pub fn check_required_inputs(recipe: &Recipe, inputs: &RecipeInputs) -> Vec<PreflightItem> {
    recipe
        .required_inputs()
        .map(|f| check_input(f, inputs.get(&f.id), true))
        .collect()
}

/// Format-validate optional + advanced inputs too (skip empty ones).
/// Baked inputs are never user-facing so are excluded.
pub fn check_optional_format(recipe: &Recipe, inputs: &RecipeInputs) -> Vec<PreflightItem> {
    let mut out = Vec::new();
    for f in recipe.optional_inputs().chain(recipe.advanced_inputs()) {
        let v = inputs.get(&f.id);
        // Optional → skip the "present" leg, only run format validation.
        if v.is_none()
            || matches!(v, Some(Value::Null))
            || matches!(v, Some(Value::String(s)) if s.is_empty())
        {
            continue;
        }
        let item = check_input(f, v, false);
        // Don't report "verified" rows for optional fields — only
        // surface them when something's off, to keep the checklist
        // focused.
        if !matches!(item.status, PreflightStatus::Verified) {
            out.push(item);
        }
    }
    out
}

fn check_input(field: &InputField, value: Option<&Value>, required: bool) -> PreflightItem {
    let id = format!("input:{}", field.id);
    let label = field.label.clone();
    let missing = match value {
        None => true,
        Some(Value::Null) => true,
        Some(Value::String(s)) if s.is_empty() => true,
        _ => false,
    };
    if missing && required {
        return PreflightItem {
            id,
            label,
            status: PreflightStatus::Blocking,
            detail: Some(format!("{} is required.", field.label)),
            fix_hint: Some(PreflightFix::FocusInput {
                input_id: field.id.clone(),
            }),
        };
    }
    if missing {
        return PreflightItem {
            id,
            label,
            status: PreflightStatus::Verified,
            detail: None,
            fix_hint: None,
        };
    }
    // Present — format validation for the typed kinds.
    if let Some(v) = value
        && let Err(detail) = validate_kind(&field.kind, v)
    {
        return PreflightItem {
            id,
            label,
            status: PreflightStatus::Blocking,
            detail: Some(detail),
            fix_hint: Some(PreflightFix::FocusInput {
                input_id: field.id.clone(),
            }),
        };
    }
    PreflightItem {
        id,
        label,
        status: PreflightStatus::Verified,
        detail: None,
        fix_hint: None,
    }
}

/// Validate a single value against an [`InputField`]'s [`FieldKind`].
///
/// Pure, no I/O — the same rules the preflight checklist enforces
/// (URL scheme, number parse, `Select` membership, `Cron` arity,
/// non-empty `Secret`). Re-used by the conversational task-setup
/// engine in `springtale-bot` to accept or reject a slot value it
/// extracted from a chat message before storing it.
pub fn validate_kind(kind: &FieldKind, value: &Value) -> Result<(), String> {
    match kind {
        FieldKind::Url => match value {
            Value::String(s) => {
                if !(s.starts_with("http://") || s.starts_with("https://")) {
                    return Err("URL must start with http:// or https://".into());
                }
                Ok(())
            }
            _ => Err("URL must be a string.".into()),
        },
        FieldKind::Number => match value {
            Value::Number(_) => Ok(()),
            Value::String(s) if s.parse::<f64>().is_ok() => Ok(()),
            Value::String(_) => Err("Expected a number.".into()),
            _ => Err("Expected a number.".into()),
        },
        FieldKind::Bool => match value {
            Value::Bool(_) => Ok(()),
            _ => Err("Expected on/off.".into()),
        },
        FieldKind::Select { options } => match value {
            Value::String(s) => {
                if options.iter().any(|o| o.value == *s) {
                    Ok(())
                } else {
                    Err(format!("'{s}' is not one of the allowed options."))
                }
            }
            _ => Err("Expected a string option.".into()),
        },
        FieldKind::Secret => match value {
            Value::String(s) if !s.is_empty() => Ok(()),
            _ => Err("Expected a non-empty secret.".into()),
        },
        FieldKind::Text => Ok(()),
        // Cron-frequency classification lives in
        // `check_schedule_frequency` (the per-recipe sweep), not in
        // per-input format validation — a cron-shape input can be
        // syntactically valid here and still fail the sweep when
        // the polling cadence is too aggressive. Format-level
        // sanity is the only thing this layer enforces.
        FieldKind::Cron => match value {
            Value::String(s) if !s.is_empty() && s.split_whitespace().count() >= 5 => Ok(()),
            Value::String(_) => {
                Err("Expected a 5- or 6-field cron expression (min hour dom month dow).".into())
            }
            _ => Err("Expected a cron expression string.".into()),
        },
        // CSS-selector inputs are validated at the picker layer
        // (Phase B); accept any non-empty string here.
        FieldKind::CssSelector { .. } => match value {
            Value::String(s) if !s.is_empty() => Ok(()),
            _ => Err("Expected a CSS selector string.".into()),
        },
        // JSON Schema inputs are validated by the schema editor at
        // submit time; accept any JSON object here.
        FieldKind::JsonSchema { .. } => match value {
            Value::Object(_) => Ok(()),
            Value::String(s) if !s.is_empty() => {
                // Authors may paste a stringified schema; accept it.
                serde_json::from_str::<serde_json::Value>(s)
                    .map(|_| ())
                    .map_err(|e| format!("JSON Schema is not valid JSON: {e}"))
            }
            _ => Err("Expected a JSON object or stringified JSON.".into()),
        },
        // D1 — Workspace target. Picker resolves the value into
        // either a raw destination_id ("12345") or a full URI
        // ("telegram://chat/12345"). Both are valid; deeper
        // verification (resolves against the directory, scheme
        // matches connector) happens in `check_workspace_target_picked`
        // which has access to the formation state. At this layer
        // we only validate that something was picked.
        FieldKind::WorkspaceTarget { .. } => match value {
            Value::String(s) if !s.is_empty() => Ok(()),
            _ => Err("Pick a destination from the dropdown.".into()),
        },
    }
}

/// Verify every connector the recipe configures is either already
/// loaded in the registry OR is a known first-party connector that
/// the apply step can register. Today the apply step calls
/// `upsert_connector_config` which can set up an unloaded connector
/// — so this check is informational rather than blocking; it's a
/// `Verified` row when the connector is already live and a `Warning`
/// otherwise so the user knows config will trigger a load.
pub async fn check_connectors(state: &RuntimeState, recipe: &Recipe) -> Vec<PreflightItem> {
    let registry = state.registry.read().await;
    recipe
        .connectors_used
        .iter()
        .map(|name| {
            let id = format!("connector_loaded:{name}");
            let label = name.clone();
            if registry.get(name).is_some() {
                PreflightItem {
                    id,
                    label,
                    status: PreflightStatus::Verified,
                    detail: Some("Already loaded.".into()),
                    fix_hint: None,
                }
            } else {
                PreflightItem {
                    id,
                    label,
                    status: PreflightStatus::Warning,
                    detail: Some("Not yet loaded — will be set up on deploy.".into()),
                    fix_hint: Some(PreflightFix::OpenConnectorConfig {
                        connector_name: name.clone(),
                    }),
                }
            }
        })
        .collect()
}

/// W4 — browser-runtime check. A recipe that drives `connector-browser`
/// needs a Chromium/Chrome binary on the host (chromiumoxide launches the
/// system browser when no `chrome_path` is configured). We mirror
/// chromiumoxide's own resolution: the `CHROME` env var, then the standard
/// per-OS install locations. Advisory (`Warning`, never `Blocking`) — the
/// user may set an explicit `chrome_path`, or install before the first run.
pub fn check_browser_runtime(recipe: &Recipe) -> Option<PreflightItem> {
    if !recipe
        .connectors_used
        .iter()
        .any(|c| c == "connector-browser")
    {
        return None;
    }

    let found = chromium_binary_path();
    Some(if let Some(path) = found {
        PreflightItem {
            id: "browser_runtime".into(),
            label: "Headless browser".into(),
            status: PreflightStatus::Verified,
            detail: Some(format!("Chromium/Chrome found at {path}.")),
            fix_hint: None,
        }
    } else {
        PreflightItem {
            id: "browser_runtime".into(),
            label: "Headless browser".into(),
            status: PreflightStatus::Warning,
            detail: Some(
                "This recipe scrapes the web, which needs Chromium or Chrome. \
                 None was found in the usual places. Install Chromium (or set the \
                 connector's chrome_path) before the first run."
                    .into(),
            ),
            fix_hint: Some(PreflightFix::Note {
                message: "Install Chromium: macOS `brew install --cask chromium`, \
                          Debian/Ubuntu `apt install chromium`, Fedora \
                          `dnf install chromium`."
                    .into(),
            }),
        }
    })
}

/// Resolve a Chromium/Chrome executable the same way chromiumoxide does:
/// the `CHROME` env var first, then the standard per-OS install paths.
fn chromium_binary_path() -> Option<String> {
    if let Ok(env_path) = std::env::var("CHROME")
        && !env_path.is_empty()
        && std::path::Path::new(&env_path).exists()
    {
        return Some(env_path);
    }
    const CANDIDATES: &[&str] = &[
        // macOS
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        // Linux
        "/usr/bin/chromium",
        "/usr/bin/chromium-browser",
        "/usr/bin/google-chrome",
        "/usr/bin/google-chrome-stable",
        "/snap/bin/chromium",
    ];
    CANDIDATES
        .iter()
        .find(|p| std::path::Path::new(p).exists())
        .map(|p| (*p).to_owned())
}

/// Cron-frequency check (Phase 0.5 / Phase A.5). Scans the recipe's
/// rule TOML for `Cron` triggers and classifies the schedule:
///
/// | Range | Status | Why |
/// |---|---|---|
/// | < 1 minute | Blocking | Source-site rate-limit hits + race conditions in the queue runner. Mirrors `apply.rs::ensure_schedule_safe`. |
/// | 1–4 minutes | Warning | Most sites consider sub-5-min polling abusive; confirm before deploy. |
/// | ≥ 5 minutes | Verified | Standard polling cadence. |
/// | ≥ 15 minutes | Verified (preferred) | Industry default (matches Zapier free tier). |
///
/// Returns one row per cron-triggered rule step in the recipe.
/// Recipes without `Cron` triggers produce no rows.
pub fn check_schedule_frequency(recipe: &Recipe, inputs: &RecipeInputs) -> Vec<PreflightItem> {
    use crate::operations::recipes::apply::substitute_template_public;
    let mut out = Vec::new();
    for (idx, step) in recipe.blueprint.rules.iter().enumerate() {
        let resolved = substitute_template_public(&step.toml, inputs);
        for expr in extract_cron_expressions_from_toml(&resolved) {
            let id = format!("schedule_freq:rule_{idx}:{expr}");
            let label = format!("Schedule {expr}");
            let item = classify_schedule(&expr, id, label);
            out.push(item);
        }
    }
    out
}

fn classify_schedule(expr: &str, id: String, label: String) -> PreflightItem {
    let bucket = ScheduleBucket::from_cron(expr);
    match bucket {
        ScheduleBucket::Invalid => PreflightItem {
            id,
            label,
            status: PreflightStatus::Blocking,
            detail: Some(format!("`{expr}` is not a valid cron expression.")),
            fix_hint: Some(PreflightFix::Note {
                message: "Use a 5-field cron expression (min hour dom month dow).".into(),
            }),
        },
        ScheduleBucket::SubMinute => PreflightItem {
            id,
            label,
            status: PreflightStatus::Blocking,
            detail: Some(format!(
                "`{expr}` fires faster than 1 minute. Raise to ≥1 minute — \
                 sub-minute schedules cause source-site rate-limit hits."
            )),
            fix_hint: Some(PreflightFix::Note {
                message: "Recommended minimum: 5 minutes for polling recipes.".into(),
            }),
        },
        ScheduleBucket::AggressivePolling => PreflightItem {
            id,
            label,
            status: PreflightStatus::Warning,
            detail: Some(format!(
                "`{expr}` polls every 1–4 minutes. Most sites consider this \
                 abusive; 5+ minutes is recommended."
            )),
            fix_hint: Some(PreflightFix::Note {
                message: "Raise to `*/5 * * * *` or higher.".into(),
            }),
        },
        ScheduleBucket::Standard => PreflightItem {
            id,
            label,
            status: PreflightStatus::Verified,
            detail: Some(format!("`{expr}` is a standard polling cadence.")),
            fix_hint: None,
        },
    }
}

#[derive(Debug, PartialEq, Eq)]
enum ScheduleBucket {
    Invalid,
    SubMinute,
    AggressivePolling,
    Standard,
}

impl ScheduleBucket {
    fn from_cron(expr: &str) -> Self {
        let fields: Vec<&str> = expr.split_whitespace().collect();
        // 6-field cron: SEC MIN HOUR DOM MON DOW.
        if fields.len() == 6 {
            let sec = fields[0];
            if sec == "*" {
                return ScheduleBucket::SubMinute;
            }
            if let Some(rest) = sec.strip_prefix("*/")
                && let Ok(n) = rest.parse::<u32>()
                && n < 60
            {
                return ScheduleBucket::SubMinute;
            }
            // SEC is fixed (e.g. "0") or `*/60+` — fall through to
            // minute-level analysis on the next field.
            return Self::classify_minute_field(fields[1]);
        }
        // 5-field cron: MIN HOUR DOM MON DOW.
        if fields.len() == 5 {
            return Self::classify_minute_field(fields[0]);
        }
        ScheduleBucket::Invalid
    }

    fn classify_minute_field(min: &str) -> Self {
        // `*/N` with N < 5 → aggressive polling.
        if let Some(rest) = min.strip_prefix("*/") {
            if let Ok(n) = rest.parse::<u32>() {
                if n == 0 {
                    return ScheduleBucket::Invalid;
                }
                if n < 5 {
                    return ScheduleBucket::AggressivePolling;
                }
                return ScheduleBucket::Standard;
            }
            return ScheduleBucket::Invalid;
        }
        // `*` (every minute) → aggressive.
        if min == "*" {
            return ScheduleBucket::AggressivePolling;
        }
        // Fixed values, ranges, lists → standard cadence (fires once
        // per matching minute in the cron schedule, which is at
        // worst once a minute).
        ScheduleBucket::Standard
    }
}

fn extract_cron_expressions_from_toml(toml: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in toml.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("expression") {
            let rest = rest.trim_start_matches(' ').trim_start_matches('=').trim();
            if let Some(stripped) = rest.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
                out.push(stripped.to_owned());
            }
        }
    }
    out
}

/// Structured-extraction capability check — if the recipe uses
/// `ExtractKind::LlmSchema`, verify the bound AI adapter supports
/// constrained decoding (i.e. `AiAdapter::structured_extractor()`
/// returns `Some`).
///
/// Catches the "AI required but the configured adapter is
/// NoopAdapter / doesn't support structured outputs" failure mode
/// at deploy time rather than runtime — same intent as
/// `check_ai_required`, narrower scope. Recipes without
/// `kind = "llm_schema"` in their rule TOML skip this check
/// entirely.
///
/// Wired here so the `feedback_preflight_zero_to_live` invariant
/// holds: a recipe that needs structured extraction never deploys
/// against an adapter that silently can't run it.
pub async fn check_structured_extraction_capability(
    state: &RuntimeState,
    recipe: &Recipe,
    inputs: &RecipeInputs,
) -> Option<PreflightItem> {
    use crate::operations::recipes::apply::substitute_template_public;

    let uses_llm_schema = recipe.blueprint.rules.iter().any(|step| {
        let resolved = substitute_template_public(&step.toml, inputs);
        contains_llm_schema_kind(&resolved)
    });
    if !uses_llm_schema {
        return None;
    }

    // Preflight has no firing context, so inspect the colony default —
    // what a rule without its own or its formation's `ai:` row resolves to.
    let adapter = state.capability_bridge.global_adapter();
    if adapter.structured_extractor().is_some() {
        return Some(PreflightItem {
            id: "ai_structured_extraction".into(),
            label: "AI structured extraction".into(),
            status: PreflightStatus::Verified,
            detail: Some("Configured AI adapter supports schema-constrained extraction.".into()),
            fix_hint: None,
        });
    }
    Some(PreflightItem {
        id: "ai_structured_extraction".into(),
        label: "AI structured extraction".into(),
        status: PreflightStatus::Blocking,
        detail: Some(
            "This recipe extracts structured data with AI, but the configured \
             provider doesn't support schema-constrained outputs. Configure \
             OpenAI (gpt-4o-2024-08-06+), Anthropic (Claude Sonnet 4+), or \
             Ollama (0.5+ with format-schema support)."
                .into(),
        ),
        fix_hint: Some(PreflightFix::OpenAiConfig),
    })
}

/// Returns `true` when the rule TOML declares an
/// `ExtractKind::LlmSchema` action. Conservative — only matches
/// `kind = "llm_schema"` inside an action's `[actions.kind]` table.
fn contains_llm_schema_kind(toml: &str) -> bool {
    let mut in_kind_block = false;
    for line in toml.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_kind_block = trimmed.ends_with(".kind]");
            continue;
        }
        if !in_kind_block {
            continue;
        }
        let normalized = trimmed.replace([' ', '"'], "");
        if normalized == "kind=llm_schema" {
            return true;
        }
    }
    false
}

/// AI-config check — if the recipe needs AI and the runtime has no
/// AI adapter configured, surface a warning with a fix that opens
/// the AI panel. `ai_required: false` recipes skip this row entirely.
pub async fn check_ai_required(state: &RuntimeState, recipe: &Recipe) -> Option<PreflightItem> {
    if !recipe.ai_required && recipe.blueprint.ai_config.is_none() {
        return None;
    }
    // If the recipe carries its own ai_config step, the apply path
    // will configure it — Verified.
    if recipe.blueprint.ai_config.is_some() {
        return Some(PreflightItem {
            id: "ai_config:bundled".into(),
            label: "AI provider".into(),
            status: PreflightStatus::Verified,
            detail: Some("Configured by the recipe.".into()),
            fix_hint: None,
        });
    }
    // Otherwise check whether the runtime already has an AI:global
    // config set.
    let raw = crate::operations::config::get_config(
        &*state.store,
        crate::operations::config::AI_COLONY_KEY,
    )
    .await
    .unwrap_or(Value::Null);
    if matches!(raw, Value::Null) {
        Some(PreflightItem {
            id: "ai_config:global".into(),
            label: "AI provider".into(),
            status: PreflightStatus::Blocking,
            detail: Some(
                "This recipe uses AI. Configure a provider (Ollama / OpenAI / Anthropic) first."
                    .into(),
            ),
            fix_hint: Some(PreflightFix::OpenAiConfig),
        })
    } else {
        Some(PreflightItem {
            id: "ai_config:global".into(),
            label: "AI provider".into(),
            status: PreflightStatus::Verified,
            detail: Some("AI provider configured.".into()),
            fix_hint: None,
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::operations::recipes::types::{
        Difficulty, RecipeBlueprint, RecipeCategory, RecipeSource,
    };
    use serde_json::json;

    fn input(id: &str, kind: FieldKind) -> InputField {
        InputField {
            id: id.into(),
            label: id.to_owned(),
            kind,
            visibility: super::super::super::recipes::types::FieldVisibility::Required,
            default: None,
            hint: None,
        }
    }

    fn recipe_with(required: Vec<InputField>) -> Recipe {
        Recipe {
            id: "r".into(),
            name: "R".into(),
            description: "".into(),
            icon_id: "".into(),
            category: RecipeCategory::Custom,
            tags: vec![],
            connectors_used: vec![],
            ai_required: false,
            difficulty: Difficulty::Quick,
            source: RecipeSource::Builtin,
            inputs: required,
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
    fn missing_required_input_is_blocking() {
        let recipe = recipe_with(vec![input("token", FieldKind::Secret)]);
        let inputs = RecipeInputs::empty();
        let items = check_required_inputs(&recipe, &inputs);
        assert_eq!(items.len(), 1);
        assert!(matches!(items[0].status, PreflightStatus::Blocking));
    }

    #[test]
    fn filled_required_secret_is_verified() {
        let recipe = recipe_with(vec![input("token", FieldKind::Secret)]);
        let mut inputs = RecipeInputs::empty();
        inputs.insert("token", json!("abc"));
        let items = check_required_inputs(&recipe, &inputs);
        assert!(matches!(items[0].status, PreflightStatus::Verified));
    }

    #[test]
    fn malformed_url_is_blocking() {
        let recipe = recipe_with(vec![input("url", FieldKind::Url)]);
        let mut inputs = RecipeInputs::empty();
        inputs.insert("url", json!("ftp://oops"));
        let items = check_required_inputs(&recipe, &inputs);
        assert!(matches!(items[0].status, PreflightStatus::Blocking));
    }

    #[test]
    fn cron_subminute_classifies_as_subminute() {
        // 6-field cron with `*` seconds field.
        assert_eq!(
            ScheduleBucket::from_cron("* * * * * *"),
            ScheduleBucket::SubMinute
        );
        // `*/30` seconds → fires twice a minute.
        assert_eq!(
            ScheduleBucket::from_cron("*/30 * * * * *"),
            ScheduleBucket::SubMinute
        );
    }

    #[test]
    fn cron_aggressive_polling_classifies_as_warning() {
        // 5-field `*` minute → every minute.
        assert_eq!(
            ScheduleBucket::from_cron("* * * * *"),
            ScheduleBucket::AggressivePolling
        );
        // `*/2` minute → every 2 minutes (still under 5).
        assert_eq!(
            ScheduleBucket::from_cron("*/2 * * * *"),
            ScheduleBucket::AggressivePolling
        );
    }

    #[test]
    fn cron_standard_classifies_5min_plus() {
        // `*/5` is exactly the recommended floor.
        assert_eq!(
            ScheduleBucket::from_cron("*/5 * * * *"),
            ScheduleBucket::Standard
        );
        // Hourly is standard.
        assert_eq!(
            ScheduleBucket::from_cron("0 * * * *"),
            ScheduleBucket::Standard
        );
        // Daily at 7am is standard.
        assert_eq!(
            ScheduleBucket::from_cron("0 7 * * *"),
            ScheduleBucket::Standard
        );
    }

    #[test]
    fn cron_invalid_classifies_as_invalid() {
        assert_eq!(ScheduleBucket::from_cron(""), ScheduleBucket::Invalid);
        assert_eq!(
            ScheduleBucket::from_cron("not a cron"),
            ScheduleBucket::Invalid
        );
        // `*/0` is a divide-by-zero authoring mistake.
        assert_eq!(
            ScheduleBucket::from_cron("*/0 * * * *"),
            ScheduleBucket::Invalid
        );
    }

    #[test]
    fn check_schedule_frequency_finds_cron_in_recipe_toml() {
        let mut recipe = recipe_with(vec![]);
        recipe
            .blueprint
            .rules
            .push(super::super::super::recipes::types::RuleStep {
                toml: r#"name = "test"

[trigger]
type = "Cron"
expression = "*/5 * * * *"

[[actions]]
type = "SendMessage"
text = "hi"
"#
                .into(),
            });
        let items = check_schedule_frequency(&recipe, &RecipeInputs::empty());
        assert_eq!(items.len(), 1);
        assert!(matches!(items[0].status, PreflightStatus::Verified));
    }

    #[test]
    fn check_schedule_frequency_warns_on_every_minute() {
        let mut recipe = recipe_with(vec![]);
        recipe
            .blueprint
            .rules
            .push(super::super::super::recipes::types::RuleStep {
                toml: r#"name = "test"

[trigger]
type = "Cron"
expression = "* * * * *"

[[actions]]
type = "SendMessage"
text = "hi"
"#
                .into(),
            });
        let items = check_schedule_frequency(&recipe, &RecipeInputs::empty());
        assert_eq!(items.len(), 1);
        assert!(
            matches!(items[0].status, PreflightStatus::Warning),
            "got {:?}",
            items[0].status
        );
    }

    #[test]
    fn check_schedule_frequency_blocks_subminute() {
        let mut recipe = recipe_with(vec![]);
        recipe
            .blueprint
            .rules
            .push(super::super::super::recipes::types::RuleStep {
                toml: r#"name = "test"

[trigger]
type = "Cron"
expression = "*/30 * * * * *"

[[actions]]
type = "SendMessage"
text = "hi"
"#
                .into(),
            });
        let items = check_schedule_frequency(&recipe, &RecipeInputs::empty());
        assert_eq!(items.len(), 1);
        assert!(matches!(items[0].status, PreflightStatus::Blocking));
    }

    #[test]
    fn check_schedule_frequency_no_cron_no_rows() {
        // Recipe with no Cron trigger produces no schedule rows.
        let mut recipe = recipe_with(vec![]);
        recipe
            .blueprint
            .rules
            .push(super::super::super::recipes::types::RuleStep {
                toml: r#"name = "test"

[trigger]
type = "ConnectorEvent"
connector = "connector-telegram"
event = "message"

[[actions]]
type = "SendMessage"
text = "hi"
"#
                .into(),
            });
        let items = check_schedule_frequency(&recipe, &RecipeInputs::empty());
        assert!(items.is_empty(), "got: {items:?}");
    }

    #[test]
    fn contains_llm_schema_kind_detects_extract_action() {
        let toml = r#"name = "test"

[trigger]
type = "Cron"
expression = "0 7 * * *"

[[actions]]
type = "Extract"
source = "last_connector_output.html"
[actions.kind]
kind = "llm_schema"
schema = { type = "object" }

[[actions]]
type = "SendMessage"
text = "${last_extract_output}"
"#;
        assert!(contains_llm_schema_kind(toml));
    }

    #[test]
    fn contains_llm_schema_kind_ignores_other_extract_kinds() {
        let toml = r#"
[[actions]]
type = "Extract"
[actions.kind]
kind = "css"
schema = { title = "h1" }
"#;
        assert!(!contains_llm_schema_kind(toml));
    }

    #[test]
    fn contains_llm_schema_kind_ignores_kind_outside_action_block() {
        // A trigger or connector with `kind = "llm_schema"` (which
        // can't exist in practice) must not trigger a false
        // positive — we only count it inside `[actions.kind]`.
        let toml = r#"
[trigger]
type = "Cron"
kind = "llm_schema"
"#;
        assert!(!contains_llm_schema_kind(toml));
    }

    #[test]
    fn select_value_must_match_option() {
        let recipe = recipe_with(vec![input(
            "mode",
            FieldKind::Select {
                options: vec![super::super::super::recipes::types::SelectOption {
                    value: "a".into(),
                    label: "A".into(),
                }],
            },
        )]);
        let mut inputs = RecipeInputs::empty();
        inputs.insert("mode", json!("bogus"));
        let items = check_required_inputs(&recipe, &inputs);
        assert!(matches!(items[0].status, PreflightStatus::Blocking));
    }
}
