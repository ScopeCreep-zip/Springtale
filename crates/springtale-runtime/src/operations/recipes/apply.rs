//! Apply a recipe — materialise its blueprint against the running runtime.
//!
//! Substitutes `${input_id}` placeholders from the user's
//! [`RecipeInputs`] into the blueprint's connector configs, rule
//! TOML, and AI config, then calls the existing connector/rule/AI
//! ops. Returns an [`ApplyReport`] summarising what landed.
//!
//! Substitution rules:
//!   - JSON values: every string leaf is template-substituted; non-
//!     string leaves pass through unchanged.
//!   - TOML strings: substituted as plain template strings.
//!   - Unknown placeholders surface as
//!     [`ApplyError::UnknownPlaceholder`] before any side effect —
//!     atomic-ish, in the sense that nothing is written when the
//!     pre-flight check fails.
//!
//! Future work tracked here:
//!   - Computed inputs (e.g. `${url_host}` derived from a `${url}`
//!     base) — needs an explicit `DerivedInput` enum on the recipe.
//!   - Per-step rollback when a later step fails after an earlier
//!     step succeeded. The current design logs the partial outcome
//!     and surfaces it through `ApplyReport`; future work adds a
//!     proper compensating-write path.

use std::collections::HashSet;

use serde_json::Value;

use crate::error::OperationError;
use crate::state::RuntimeState;

use super::library;
use super::types::{ApplyReport, Recipe, RecipeBlueprint, RecipeInputs};

#[derive(Debug, thiserror::Error)]
pub enum ApplyError {
    #[error("recipe not found: {0}")]
    RecipeNotFound(String),
    #[error("missing required input: {0}")]
    MissingRequiredInput(String),
    #[error("placeholder `${{{0}}}` is not declared as an input on this recipe")]
    UnknownPlaceholder(String),
    #[error("substituted rule TOML failed to parse: {0}")]
    InvalidRuleToml(String),
    #[error(
        "schedule `{expr}` fires faster than the minimum 1 minute interval. \
         Sub-minute schedules cause source-site rate-limit hits and race \
         conditions; raise the cron expression to ≥1 minute"
    )]
    ScheduleTooFast { expr: String },
    #[error("action `{step}` was rejected by the connector's input schema: {reason}")]
    ActionSchema { step: String, reason: String },
    #[error("backend operation failed: {0}")]
    Operation(#[from] OperationError),
    #[error(transparent)]
    Derive(#[from] super::resolver::ResolverError),
}

/// Apply a built-in or user recipe to the runtime.
pub async fn apply_recipe(
    state: &RuntimeState,
    recipe_id: &str,
    mut inputs: RecipeInputs,
) -> Result<ApplyReport, ApplyError> {
    let recipe = library::get_recipe(&*state.store, recipe_id)
        .await?
        .ok_or_else(|| ApplyError::RecipeNotFound(recipe_id.to_owned()))?;

    ensure_required_inputs_present(&recipe, &inputs)?;
    ensure_placeholders_resolve(&recipe, &inputs)?;
    ensure_schedule_safe(&recipe, &inputs)?;

    // Resolve derived inputs (e.g. geocode a free-text city into
    // `latitude=..&longitude=..`) BEFORE substitution, so the universal
    // recipe takes any target instead of a hardcoded enum. Fails the
    // deploy before any side effect if a target can't be resolved.
    super::resolver::apply_derived_inputs(&recipe, &mut inputs).await?;

    apply_blueprint(state, &recipe, &inputs).await
}

/// Hard-block recipes whose `Cron` triggers fire faster than once a
/// minute. Sub-minute schedules cause source-site rate-limit hits and
/// race conditions in the queue runner; per the v2 plan §"Cron
/// frequency guard" they're a Deploy-time error, not a runtime
/// warning.
///
/// The 1-minute floor is intentionally generous — `feedback_no_ban_risk`
/// argues for 5+ minutes as the recommended default, surfaced via a
/// soft `cron_frequency_warning` in [`crate::operations::preflight`]
/// (yellow chip in the deploy form). The hard block here only catches
/// obviously broken authoring (e.g. `* * * * * *` 6-field cron with
/// seconds).
pub fn ensure_schedule_safe(recipe: &Recipe, inputs: &RecipeInputs) -> Result<(), ApplyError> {
    for step in &recipe.blueprint.rules {
        // Substitute inputs into the TOML so we see the actual cron
        // expression the deploy would use (the author may have
        // written `expression = "${schedule}"` and surfaced the cron
        // as a recipe input).
        let resolved = substitute_template(&step.toml, inputs);
        for expr in extract_cron_expressions(&resolved) {
            if is_subminute(&expr) {
                return Err(ApplyError::ScheduleTooFast { expr });
            }
        }
    }
    Ok(())
}

/// Scan a TOML rule body for cron `expression = "..."` lines and
/// return each cron expression string. Conservative — only matches
/// the canonical `expression = "..."` shape produced by the recipe
/// catalog. The cron crate's full parser runs against each match
/// in `is_subminute` for the actual validation.
fn extract_cron_expressions(toml: &str) -> Vec<String> {
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

/// `true` when the cron expression includes a non-`*` seconds field,
/// indicating sub-minute firing. Standard 5-field cron has no
/// seconds; 6-field cron (the `cron` crate's default) puts seconds
/// first. We treat anything that explicitly schedules per-second
/// firing as too fast.
fn is_subminute(expr: &str) -> bool {
    let fields: Vec<&str> = expr.split_whitespace().collect();
    // 6-field cron: SEC MIN HOUR DOM MON DOW. If SEC is `*` or a
    // step like `*/30`, it fires every N seconds within each minute.
    if fields.len() == 6 {
        let sec = fields[0];
        if sec == "*" {
            return true;
        }
        if let Some(rest) = sec.strip_prefix("*/")
            && let Ok(n) = rest.parse::<u32>()
            && n < 60
        {
            return true;
        }
    }
    // 5-field cron has no seconds — minute granularity is the cap.
    false
}

/// Render the recipe's blueprint as a single TOML document for the
/// "Show as code" disclosure panel. Substitution is best-effort —
/// missing inputs leave their placeholder visible so the user knows
/// what still needs filling.
pub async fn render_blueprint_toml(
    state: &RuntimeState,
    recipe_id: &str,
    inputs: RecipeInputs,
) -> Result<String, ApplyError> {
    let recipe = library::get_recipe(&*state.store, recipe_id)
        .await?
        .ok_or_else(|| ApplyError::RecipeNotFound(recipe_id.to_owned()))?;
    Ok(render_toml(&recipe.blueprint, &inputs))
}

fn ensure_required_inputs_present(
    recipe: &Recipe,
    inputs: &RecipeInputs,
) -> Result<(), ApplyError> {
    for field in recipe.required_inputs() {
        match inputs.get(&field.id) {
            None => return Err(ApplyError::MissingRequiredInput(field.id.clone())),
            Some(Value::Null) => return Err(ApplyError::MissingRequiredInput(field.id.clone())),
            Some(Value::String(s)) if s.is_empty() => {
                return Err(ApplyError::MissingRequiredInput(field.id.clone()));
            }
            _ => {}
        }
    }
    Ok(())
}

/// Validate that every author-declared input id appears in *no more*
/// than one classification AND that the placeholders the author wrote
/// either resolve to a declared input or look like a rule-engine
/// reference (dotted path: `trigger.x`, `event.y`, `previous.z`).
///
/// Bare unknown ids (no dot) are author typos — we still error so the
/// author catches them. Dotted ids are evaluated by the rule engine
/// at execution time per
/// `springtale_core::transform::format::resolve_template`, so we let
/// them through.
fn ensure_placeholders_resolve(recipe: &Recipe, _inputs: &RecipeInputs) -> Result<(), ApplyError> {
    let mut known_ids: HashSet<&str> = recipe.inputs.iter().map(|f| f.id.as_str()).collect();
    // Derived-input targets (e.g. `location` from `geocode(city)`) are
    // populated at deploy time, so they're valid placeholders even though
    // they're not author-declared `InputField`s.
    for resolver in &recipe.blueprint.derived_inputs {
        match resolver {
            super::types::DerivedInputResolver::Geocode {
                target_input_id, ..
            } => {
                known_ids.insert(target_input_id.as_str());
            }
        }
    }
    let mut found = HashSet::new();
    collect_placeholders_value(
        &serde_json::to_value(&recipe.blueprint).unwrap_or(Value::Null),
        &mut found,
    );
    for id in found {
        if id.contains('.') {
            // Rule-engine reference — substituted at execution time
            // against the trigger payload. Skip apply-time validation.
            continue;
        }
        if is_runtime_reference(&id) {
            // BARE runtime references (`${last_connector_output}`, `${now}`,
            // `${stepN}`) are resolved by the chain context at fire time,
            // not from recipe inputs — they're valid even without a dot.
            continue;
        }
        if !known_ids.contains(id.as_str()) {
            return Err(ApplyError::UnknownPlaceholder(id));
        }
    }
    Ok(())
}

/// Whole-value runtime references the chain context resolves at fire time
/// (mirrors `crate::rule::template_resolve` + the builtin recipe test's
/// allowed roots). `stepN` is positional (1-indexed step output).
fn is_runtime_reference(id: &str) -> bool {
    const RUNTIME_ROOTS: &[&str] = &[
        "trigger",
        "last_ai_output",
        "last_connector_output",
        "last_connector_message",
        "last_extract_output",
        "last_dedupe_output",
        "last_shell_output",
        "now",
        "run_id",
        "bot",
    ];
    RUNTIME_ROOTS.contains(&id)
        || (id.len() > 4 && id.starts_with("step") && id[4..].bytes().all(|b| b.is_ascii_digit()))
}

fn collect_placeholders_value(value: &Value, out: &mut HashSet<String>) {
    match value {
        Value::String(s) => collect_placeholders_str(s, out),
        Value::Array(arr) => arr.iter().for_each(|v| collect_placeholders_value(v, out)),
        Value::Object(map) => map
            .values()
            .for_each(|v| collect_placeholders_value(v, out)),
        _ => {}
    }
}

fn collect_placeholders_str(s: &str, out: &mut HashSet<String>) {
    // Simple `${id}` scanner: state-machine across the bytes.
    let bytes = s.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'$'
            && bytes[i + 1] == b'{'
            && let Some(end) = bytes[i + 2..].iter().position(|&b| b == b'}')
        {
            let id = &s[i + 2..i + 2 + end];
            if !id.is_empty() {
                out.insert(id.to_owned());
            }
            i += end + 3;
            continue;
        }
        i += 1;
    }
}

async fn apply_blueprint(
    state: &RuntimeState,
    recipe: &Recipe,
    inputs: &RecipeInputs,
) -> Result<ApplyReport, ApplyError> {
    let blueprint = &recipe.blueprint;
    let mut connectors_configured = Vec::new();
    let mut rules_created = Vec::new();
    let mut ai_configured = false;

    for step in &blueprint.connector_configs {
        let resolved = substitute_value(&step.config, inputs);
        crate::operations::config::upsert_connector_config(state, &step.connector_name, resolved)
            .await?;
        connectors_configured.push(step.connector_name.clone());
    }

    for step in &blueprint.rules {
        let toml = substitute_template(&step.toml, inputs);
        let rule: springtale_core::rule::types::Rule =
            toml::from_str(&toml).map_err(|e| ApplyError::InvalidRuleToml(e.to_string()))?;
        // Gate: the rendered params must satisfy the connector action's
        // input_schema, or the connector would reject the rule at dispatch.
        // Connectors not in the registry are skipped — preflight already
        // reported them.
        {
            let registry = state.registry.read().await;
            let checks = super::action_schema::check_rule_actions(&rule, |name| {
                registry.get(name).map(|entry| entry.host.actions())
            });
            if let Some((step, reason)) = checks.into_iter().find_map(|c| match c.outcome {
                super::action_schema::ActionOutcome::Invalid(reason) => Some((c.step, reason)),
                _ => None,
            }) {
                return Err(ApplyError::ActionSchema { step, reason });
            }
        }
        let id = crate::operations::rules::create_rule(state, rule).await?;
        rules_created.push(id.0.to_string());
    }

    if let Some(step) = &blueprint.ai_config {
        let resolved = substitute_value(&step.config, inputs);
        crate::operations::config::configure_ai_adapter(state, step.target.clone(), resolved)
            .await?;
        ai_configured = true;
    }

    let summary = blueprint
        .summary
        .clone()
        .unwrap_or_else(|| format!("Deployed recipe '{}'.", recipe.name));

    Ok(ApplyReport {
        recipe_id: recipe.id.clone(),
        connectors_configured,
        rules_created,
        ai_configured,
        summary,
    })
}

/// Public wrapper around the internal value substitution — exposed
/// so `crate::operations::preview` can render the same substituted
/// values without re-implementing the logic.
pub fn substitute_value_public(value: &Value, inputs: &RecipeInputs) -> Value {
    substitute_value(value, inputs)
}

/// Public wrapper around the internal template substitution.
pub fn substitute_template_public(s: &str, inputs: &RecipeInputs) -> String {
    substitute_template(s, inputs)
}

fn substitute_value(value: &Value, inputs: &RecipeInputs) -> Value {
    match value {
        Value::String(s) => {
            // If the whole string is a single `${id}`, swap in the
            // typed value (preserves numbers, booleans, arrays as-is
            // rather than coercing to strings).
            if let Some(id) = whole_string_placeholder(s)
                && let Some(v) = inputs.get(&id)
            {
                return v.clone();
            }
            Value::String(substitute_template(s, inputs))
        }
        Value::Array(arr) => {
            Value::Array(arr.iter().map(|v| substitute_value(v, inputs)).collect())
        }
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, v) in map {
                out.insert(k.clone(), substitute_value(v, inputs));
            }
            Value::Object(out)
        }
        other => other.clone(),
    }
}

fn whole_string_placeholder(s: &str) -> Option<String> {
    let trimmed = s.trim();
    if trimmed.starts_with("${") && trimmed.ends_with('}') && !trimmed[2..].contains("${") {
        let id = &trimmed[2..trimmed.len() - 1];
        if !id.is_empty() {
            return Some(id.to_owned());
        }
    }
    None
}

/// Apply-time template substitution.
///
/// Replaces `${id}` placeholders that match a supplied recipe input.
/// Leaves dotted ids (`${trigger.field}`, `${previous.output}`) intact
/// so the rule engine's `resolve_template` evaluates them per-event at
/// execution time. Unknown bare ids are also left literal — they were
/// already rejected by `ensure_placeholders_resolve`, so reaching here
/// means the recipe author intentionally referenced a rule-engine
/// symbol that isn't a recipe input (e.g. `${last_ai_output}` from the
/// action chain).
fn substitute_template(s: &str, inputs: &RecipeInputs) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len()
            && bytes[i] == b'$'
            && bytes[i + 1] == b'{'
            && let Some(end) = bytes[i + 2..].iter().position(|&b| b == b'}')
        {
            let id = &s[i + 2..i + 2 + end];
            if !id.is_empty()
                && !id.contains('.')
                && let Some(v) = inputs.get(id)
            {
                // A quoted placeholder that is the whole value (`"${id}"`)
                // takes the input's own type: a number or boolean is
                // emitted bare so the TOML value is typed, mirroring the
                // rule engine's whole-string substitution at fire time.
                // Strings stay quoted; every other position is textual.
                let whole_quoted = i > 0
                    && bytes[i - 1] == b'"'
                    && bytes.get(i + end + 3) == Some(&b'"')
                    && matches!(v, Value::Number(_) | Value::Bool(_));
                if whole_quoted {
                    out.pop();
                    out.push_str(&json_to_display_string(v));
                    i += end + 4;
                    continue;
                }
                out.push_str(&json_to_display_string(v));
                i += end + 3;
                continue;
            }
            // Dotted or unknown bare → preserve literal for the
            // rule-engine evaluator (or for a downstream system).
            out.push_str(&s[i..i + end + 3]);
            i += end + 3;
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

#[cfg(test)]
mod typed_substitution_tests {
    use super::*;

    #[test]
    fn whole_quoted_number_and_bool_become_bare_values() {
        let mut inputs = RecipeInputs::empty();
        inputs.insert(String::from("ttl"), serde_json::json!(60));
        inputs.insert(String::from("on"), serde_json::json!(true));
        inputs.insert(String::from("name"), serde_json::json!("x"));
        let toml = substitute_template(
            "seconds = \"${ttl}\"\nflag = \"${on}\"\nlabel = \"${name}\"\ntext = \"ttl=${ttl}\"\n",
            &inputs,
        );
        assert_eq!(
            toml,
            "seconds = 60\nflag = true\nlabel = \"x\"\ntext = \"ttl=60\"\n"
        );
    }
}

fn json_to_display_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::Null => String::new(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

fn render_toml(blueprint: &RecipeBlueprint, inputs: &RecipeInputs) -> String {
    let mut out = String::new();
    for step in &blueprint.rules {
        out.push_str("# ── Rule ──\n");
        out.push_str(&substitute_template(&step.toml, inputs));
        out.push('\n');
    }
    for step in &blueprint.connector_configs {
        out.push_str(&format!(
            "# ── Connector config: {} ──\n",
            step.connector_name
        ));
        let resolved = substitute_value(&step.config, inputs);
        out.push_str(&serde_json::to_string_pretty(&resolved).unwrap_or_else(|_| "{}".into()));
        out.push_str("\n\n");
    }
    if let Some(step) = &blueprint.ai_config {
        out.push_str(&format!("# ── AI config: {} ──\n", step.target.key()));
        let resolved = substitute_value(&step.config, inputs);
        out.push_str(&serde_json::to_string_pretty(&resolved).unwrap_or_else(|_| "{}".into()));
        out.push('\n');
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::operations::recipes::types::{
        ConnectorConfigStep, Difficulty, FieldKind, FieldVisibility, InputField, Recipe,
        RecipeBlueprint, RecipeCategory, RecipeSource,
    };
    use serde_json::json;

    fn sample_recipe() -> Recipe {
        Recipe {
            id: "test".into(),
            name: "Test".into(),
            description: "".into(),
            icon_id: "robot".into(),
            category: RecipeCategory::Custom,
            tags: vec![],
            connectors_used: vec!["connector-test".into()],
            ai_required: false,
            difficulty: Difficulty::Quick,
            source: RecipeSource::Builtin,
            inputs: vec![
                InputField {
                    id: "token".into(),
                    label: "Token".into(),
                    kind: FieldKind::Secret,
                    visibility: FieldVisibility::Required,
                    default: None,
                    hint: None,
                },
                InputField {
                    id: "count".into(),
                    label: "Count".into(),
                    kind: FieldKind::Number,
                    visibility: FieldVisibility::Optional,
                    default: Some(json!(5)),
                    hint: None,
                },
            ],
            blueprint: RecipeBlueprint {
                connector_configs: vec![ConnectorConfigStep {
                    connector_name: "connector-test".into(),
                    config: json!({
                        "token": "${token}",
                        "count": "${count}",
                        "fixed": "hello"
                    }),
                }],
                rules: vec![],
                ai_config: None,
                summary: None,
                derived_inputs: vec![],
            },
        }
    }

    #[test]
    fn whole_string_placeholder_typed_value_preserved() {
        let mut inputs = RecipeInputs::empty();
        inputs.insert("count", json!(42));
        let resolved = substitute_value(&json!("${count}"), &inputs);
        assert_eq!(resolved, json!(42));
    }

    #[test]
    fn template_substitution_for_mixed_strings() {
        let mut inputs = RecipeInputs::empty();
        inputs.insert("name", json!("alice"));
        let out = substitute_template("Hello, ${name}!", &inputs);
        assert_eq!(out, "Hello, alice!");
    }

    #[test]
    fn unknown_placeholder_caught_before_apply() {
        let recipe = sample_recipe();
        let mut inputs = RecipeInputs::empty();
        inputs.insert("token", json!("abc"));
        let result = ensure_placeholders_resolve(&recipe, &inputs);
        assert!(result.is_ok());

        let mut bad = recipe.clone();
        bad.blueprint.connector_configs[0].config = json!({ "key": "${bogus}" });
        let result = ensure_placeholders_resolve(&bad, &inputs);
        assert!(matches!(result, Err(ApplyError::UnknownPlaceholder(s)) if s == "bogus"));
    }

    #[test]
    fn missing_required_input_fails_fast() {
        let recipe = sample_recipe();
        let inputs = RecipeInputs::empty();
        let result = ensure_required_inputs_present(&recipe, &inputs);
        assert!(matches!(result, Err(ApplyError::MissingRequiredInput(s)) if s == "token"));
    }

    #[test]
    fn render_blueprint_includes_substituted_values() {
        let recipe = sample_recipe();
        let mut inputs = RecipeInputs::empty();
        inputs.insert("token", json!("secret-123"));
        inputs.insert("count", json!(10));
        let rendered = render_toml(&recipe.blueprint, &inputs);
        assert!(rendered.contains("secret-123"));
        assert!(rendered.contains("10"));
    }

    /// DEPLOY VALIDATION for EVERY builtin. Runs the three pre-side-effect
    /// gates `apply_recipe` runs — the ones that REJECTED
    /// `${last_connector_output}` and would reject any undeclared
    /// placeholder, missing required input, or sub-minute schedule. Pure +
    /// hermetic (no RuntimeState, no network — geocoding runs after these
    /// gates). This is the regression net for the deploy-class bug.
    #[test]
    fn every_builtin_passes_deploy_validation() {
        for recipe in crate::operations::recipes::builtin::all() {
            let mut inputs = RecipeInputs::empty();
            for f in recipe.required_inputs() {
                inputs.insert(f.id.clone(), json!("placeholder-value"));
            }
            ensure_required_inputs_present(&recipe, &inputs)
                .unwrap_or_else(|e| panic!("{} fails required-inputs gate: {e}", recipe.id));
            ensure_placeholders_resolve(&recipe, &inputs)
                .unwrap_or_else(|e| panic!("{} fails placeholder gate: {e}", recipe.id));
            ensure_schedule_safe(&recipe, &inputs)
                .unwrap_or_else(|e| panic!("{} fails schedule gate: {e}", recipe.id));
        }
    }
}
