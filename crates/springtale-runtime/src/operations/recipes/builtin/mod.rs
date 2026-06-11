//! Built-in recipes — the starter library compiled into the binary.
//!
//! Each recipe is a typed click-and-play wrapper around the existing
//! `Action::*` / `Trigger::*` / connector primitives. The CLI's
//! `springtale new <template>` writes a project to disk; recipes
//! materialise the same intent directly into the running runtime so
//! the UI never asks the user to edit TOML on a filesystem.
//!
//! Organised by [`super::types::RecipeCategory`] into one submodule
//! per category. Adding a new built-in:
//!   1. Decide which category file the recipe belongs in.
//!   2. Define the [`super::types::Recipe`] in a `pub fn` in that
//!      file. Author declares `FieldVisibility` per input — no
//!      heuristic. See `messaging::telegram_echo` for the
//!      canonical example.
//!   3. Register the function in that file's `pub fn all() ->
//!      Vec<Recipe>`.
//!   4. The invariant tests at the bottom of this file run on every
//!      built-in (unique id, valid TOML, declared visibility, etc.).
//!
//! See `feedback_universal_design_from_bottom` + the catalogue plan:
//! recipes are designed for the most-threatened user (Trans Army /
//! EFF / CETA / Activist Handbook guidance) and made universally
//! useful — no persona splits in the UI.

pub mod ai_assistant;
pub mod coding;
pub mod daily;
pub mod messaging;
pub mod safety_privacy;
pub mod universal;
pub mod web;

use super::types::Recipe;

/// Every built-in recipe, chained from each category file in display
/// order: Messaging → Coding → Web → AiAssistant → Daily →
/// SafetyPrivacy → Universal. The order matters for the default
/// `RecipeSort::Recommended` view in `library::list_recipes`.
/// Universal sits last so its parametrized shapes don't out-rank the
/// content-specific recipes in search; users who know what they want
/// find the targeted recipe first, users who want flexibility find
/// the universal shapes when narrower options don't match.
pub fn all() -> Vec<Recipe> {
    let mut out = Vec::new();
    out.extend(messaging::all());
    out.extend(coding::all());
    out.extend(web::all());
    out.extend(ai_assistant::all());
    out.extend(daily::all());
    out.extend(safety_privacy::all());
    out.extend(universal::all());
    out
}

/// Fetch one by id. Linear search across the full catalogue — the
/// catalogue is small (~50 recipes) so the constant factor is fine
/// and avoids a startup-time map allocation.
pub fn get(id: &str) -> Option<Recipe> {
    all().into_iter().find(|r| r.id == id)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::operations::recipes::types::{FieldVisibility, RecipeSource};

    #[test]
    fn every_builtin_has_unique_id() {
        let mut seen = std::collections::HashSet::new();
        for r in all() {
            assert!(seen.insert(r.id.clone()), "duplicate id: {}", r.id);
        }
    }

    #[test]
    fn every_builtin_is_builtin_source() {
        for r in all() {
            assert!(matches!(r.source, RecipeSource::Builtin));
        }
    }

    #[test]
    fn every_builtin_blueprint_has_at_least_one_step() {
        for r in all() {
            let has_step = !r.blueprint.connector_configs.is_empty()
                || !r.blueprint.rules.is_empty()
                || r.blueprint.ai_config.is_some();
            assert!(has_step, "{} has no blueprint steps", r.id);
        }
    }

    #[test]
    fn get_returns_known_recipe() {
        let r = get("telegram-echo").expect("telegram-echo missing");
        assert_eq!(r.name, "Telegram Echo");
    }

    #[test]
    fn get_returns_none_for_unknown() {
        assert!(get("nonexistent-recipe").is_none());
    }

    /// Every built-in's rule TOML must parse as a `Rule` directly,
    /// with no normaliser bridge. This is the load-bearing test for
    /// the "modular shape IS the canonical shape" architecture — a
    /// future contributor adding a recipe in the old `[rule]`-
    /// wrapper dialect or with a wrong action-variant tag will fail
    /// here at `cargo test` time, not at the user's machine.
    #[test]
    fn every_builtin_rule_toml_parses_canonically() {
        use crate::operations::recipes::apply::substitute_template_public;
        use crate::operations::recipes::types::RecipeInputs;
        use springtale_core::rule::types::Rule;
        for recipe in all() {
            let mut inputs = RecipeInputs::empty();
            for f in &recipe.inputs {
                inputs.insert(f.id.clone(), serde_json::json!("placeholder"));
            }
            for step in &recipe.blueprint.rules {
                let toml = substitute_template_public(&step.toml, &inputs);
                let parsed: Result<Rule, _> = toml::from_str(&toml);
                assert!(
                    parsed.is_ok(),
                    "recipe {} rule TOML failed canonical parse: {}\n--- toml ---\n{}",
                    recipe.id,
                    parsed.err().map(|e| e.to_string()).unwrap_or_default(),
                    toml,
                );
            }
        }
    }

    /// Author-declared visibility — every built-in must declare each
    /// field's visibility explicitly. Catches regressions where a new
    /// recipe is added without thinking about which tier each field
    /// belongs to.
    #[test]
    fn every_builtin_input_has_explicit_visibility() {
        for recipe in all() {
            for f in &recipe.inputs {
                match f.visibility {
                    FieldVisibility::Required
                    | FieldVisibility::Optional
                    | FieldVisibility::Advanced
                    | FieldVisibility::Baked => (),
                }
            }
        }
    }

    /// The catalogue must keep growing without shrinking. Threshold
    /// reflects the W2 catalogue-expansion plan: existing 8 + the
    /// ~42 new recipes across the 6 category files = at least 50.
    #[test]
    fn catalogue_has_at_least_50_recipes() {
        let count = all().len();
        assert!(
            count >= 50,
            "expected ≥50 built-in recipes, found {count}. Add new recipes to the appropriate category file under `builtin/`."
        );
    }

    /// Every `${...}` placeholder remaining in a recipe's rule TOML
    /// after apply-time substitution must reference a known
    /// runtime-state path that the dispatcher can resolve via
    /// [`springtale_core::rule::template_resolve`]. Pre-Phase-0 the
    /// dispatcher had no path that resolved `${last_ai_output}` /
    /// `${last_connector_output}` — 12 shipped recipes posted the
    /// literal strings to users. This invariant locks in the fix.
    ///
    /// Allowed runtime references (first dot-separated token):
    ///   - `trigger` (trigger payload)
    ///   - `last_ai_output`, `last_connector_output`,
    ///     `last_extract_output`, `last_dedupe_output`
    ///   - `stepN` (positional, 1-indexed)
    ///   - `step` (named, `step.NAME.field`)
    ///   - `now`, `run_id`, `bot`
    ///
    /// Catches: typos (`last_ai_outputt`), stale references after a
    /// chain refactor, and recipe authors writing apply-time
    /// placeholders (`${chat_id}`) where they meant trigger payload
    /// (`${trigger.chat_id}`).
    #[test]
    fn every_runtime_state_placeholder_resolves_in_synthetic_chain() {
        use crate::operations::recipes::apply::substitute_template_public;
        use crate::operations::recipes::types::RecipeInputs;
        use regex::Regex;

        let allowed_roots: std::collections::HashSet<&str> = [
            "trigger",
            "last_ai_output",
            "last_connector_output",
            "last_connector_message",
            "last_extract_output",
            "last_dedupe_output",
            "last_shell_output",
            "step",
            "now",
            "run_id",
            "bot",
        ]
        .into_iter()
        .collect();

        // Matches `stepN` where N is one or more digits.
        let step_indexed = Regex::new(r"^step\d+$").unwrap();
        // Matches the placeholder body inside `${...}`.
        let placeholder = Regex::new(r"\$\{([^}]+)\}").unwrap();

        for recipe in all() {
            // Synthetic apply-time inputs — every declared input gets
            // a value so substitute_template_public consumes every
            // `${input_id}` placeholder. Whatever survives must be a
            // runtime-state reference.
            let mut inputs = RecipeInputs::empty();
            for f in &recipe.inputs {
                inputs.insert(f.id.clone(), serde_json::json!("placeholder"));
            }
            // Derived inputs (e.g. `location` from `geocode(city)`) are
            // resolved at apply time too, so the input layer consumes their
            // `${target}` placeholders. Inject synthetic values for them.
            for resolver in &recipe.blueprint.derived_inputs {
                match resolver {
                    crate::operations::recipes::types::DerivedInputResolver::Geocode {
                        target_input_id,
                        ..
                    } => {
                        inputs.insert(target_input_id.clone(), serde_json::json!("placeholder"));
                    }
                }
            }

            for step in &recipe.blueprint.rules {
                let resolved_toml = substitute_template_public(&step.toml, &inputs);

                for cap in placeholder.captures_iter(&resolved_toml) {
                    let body = &cap[1];
                    let root = body.split('.').next().unwrap_or(body);

                    let is_step_indexed = step_indexed.is_match(root);
                    let is_known_root = allowed_roots.contains(root);

                    assert!(
                        is_known_root || is_step_indexed,
                        "recipe `{recipe_id}` leaves an unresolved placeholder \
                         `${{{body}}}` (root `{root}`) after apply-time \
                         substitution. Allowed runtime roots: trigger, \
                         last_ai_output, last_connector_output, \
                         last_extract_output, last_dedupe_output, stepN, \
                         step.NAME, now, run_id, bot. If this is a typo, fix \
                         the recipe. If it's meant to be an apply-time input, \
                         declare it in `recipe.inputs` so the input layer \
                         consumes it.\n--- resolved TOML ---\n{resolved_toml}",
                        recipe_id = recipe.id,
                    );
                }
            }
        }
    }

    /// OUTPUT-TRANSFORM CONTRACT (skeleton check). A recipe must never
    /// hand a person — or an AI prompt — a RAW data blob. Every value
    /// surfaced to a user (`Notify.body`, `SendMessage`, a messaging
    /// `send_*` text) or fed to `AiComplete` must be a formatted message
    /// or a specific extracted field, never the whole connector output /
    /// raw response body / whole extract object. The deterministic fix is
    /// an `Extract` step + `${last_extract_output.FIELD}`. This is the
    /// output-quality analogue of the placeholder test above — the check
    /// that would have caught the weather raw-JSON bug.
    #[test]
    fn no_recipe_hands_a_raw_blob_to_a_user_or_ai() {
        use crate::operations::recipes::apply::substitute_template_public;
        use crate::operations::recipes::types::RecipeInputs;
        use springtale_core::rule::action::Action;
        use springtale_core::rule::types::Rule;

        // What a USER may never be shown: ANY raw data (whole connector
        // envelope, raw response `.body`/`.output`, whole extract object,
        // gated shell stub). `${last_ai_output}` / `${last_connector_message}`
        // are allowed — already-formatted prose / human messages.
        const RAW_USER: &[&str] = &[
            "${last_connector_output}",
            "${last_connector_output.body}",
            "${last_connector_output.output}",
            "${last_shell_output}",
            "${last_extract_output}",
        ];
        // What an AI PROMPT may never be fed: the whole HTTP envelope (status+
        // headers noise) or the gated shell stub. Feeding `.body` (the response
        // TEXT the model should read) is fine — the model reads text.
        const RAW_AI: &[&str] = &["${last_connector_output}", "${last_shell_output}"];

        // (text, is_user_facing) — AiComplete prompts are AI-facing, the rest user-facing.
        fn collect(action: &Action, out: &mut Vec<(String, bool)>) {
            match action {
                Action::Notify { body, .. } => out.push((body.clone(), true)),
                Action::SendMessage { text } => out.push((text.clone(), true)),
                Action::AiComplete { prompt, .. } => out.push((prompt.clone(), false)),
                Action::RunConnector { action, params, .. } if action.starts_with("send") => {
                    for key in ["text", "message", "body"] {
                        if let Some(s) = params.get(key).and_then(|v| v.as_str()) {
                            out.push((s.to_owned(), true));
                        }
                    }
                }
                Action::Chain { steps } => {
                    for s in steps {
                        collect(s, out);
                    }
                }
                _ => {}
            }
        }

        let mut violations: Vec<String> = Vec::new();
        for recipe in all() {
            let mut inputs = RecipeInputs::empty();
            for f in &recipe.inputs {
                inputs.insert(f.id.clone(), serde_json::json!("x"));
            }
            for rule_step in &recipe.blueprint.rules {
                let toml = substitute_template_public(&rule_step.toml, &inputs);
                let Ok(rule) = toml::from_str::<Rule>(&toml) else {
                    continue;
                };
                let mut surfaced = Vec::new();
                for a in &rule.actions {
                    collect(a, &mut surfaced);
                }
                for (body, user_facing) in surfaced {
                    let raws = if user_facing { RAW_USER } else { RAW_AI };
                    for raw in raws {
                        if body.contains(raw) {
                            let dest = if user_facing {
                                "to a user"
                            } else {
                                "to an AI prompt"
                            };
                            violations.push(format!("{} surfaces raw `{raw}` {dest}", recipe.id));
                        }
                    }
                }
            }
        }

        assert!(
            violations.is_empty(),
            "recipes hand raw data to a user/AI — add an `Extract` step and \
             reference `${{last_extract_output.FIELD}}` instead:\n{}",
            violations.join("\n"),
        );
    }

    /// END-TO-END SUCCESS (no AI). Fire the weather recipe's chain with a
    /// realistic stubbed connector response and assert the user sees a
    /// human sentence — the actual `Extract` + template-resolve code, the
    /// thing the parse tests never exercised. This is the test that proves
    /// the recipe WORKS (not just parses), with a NoopAdapter (no AI).
    #[tokio::test]
    async fn weather_recipe_fires_with_human_readable_output() {
        use crate::extraction::extract;
        use crate::operations::recipes::apply::substitute_template_public;
        use crate::operations::recipes::types::RecipeInputs;
        use springtale_core::rule::action::Action;
        use springtale_core::rule::chain_context::ChainContext;
        use springtale_core::rule::template_resolve::resolve_chain_template;
        use springtale_core::rule::types::Rule;

        // Realistic Open-Meteo forecast — `body` is the raw response TEXT.
        let body = serde_json::json!({
            "current": {
                "temperature_2m": 72.4,
                "apparent_temperature": 70.1,
                "wind_speed_10m": 6.3
            }
        })
        .to_string();

        let recipe = get("weather-briefing").expect("weather recipe");
        let mut inputs = RecipeInputs::empty();
        inputs.insert("city", serde_json::json!("Sacramento, CA"));
        inputs.insert("schedule", serde_json::json!("0 8 * * *"));
        inputs.insert("location", serde_json::json!("latitude=38&longitude=-121"));

        let toml = substitute_template_public(&recipe.blueprint.rules[0].toml, &inputs);
        let rule: Rule = toml::from_str(&toml).expect("rule parses");
        let Action::Chain { steps } = &rule.actions[0] else {
            panic!("expected a Chain action");
        };

        let mut chain = ChainContext::new(serde_json::Value::Null);
        chain.last_connector_output =
            Some(serde_json::json!({ "status": 200, "headers": {}, "body": body }));

        let mut final_msg = String::new();
        for step in steps {
            match step {
                Action::Extract { source, kind } => {
                    assert_eq!(source, "last_connector_output.body");
                    let src = chain
                        .last_connector_output
                        .as_ref()
                        .and_then(|v| v.get("body"))
                        .cloned()
                        .expect("connector body");
                    let out = extract(&src, kind, None).await.expect("extract ok");
                    chain.last_extract_output = Some(out);
                }
                Action::Notify { body, .. } => {
                    final_msg = resolve_chain_template(body, &chain, None);
                }
                _ => {}
            }
        }

        // The user sees a sentence — not JSON, not placeholders, not null.
        assert!(final_msg.contains("°F"), "no temperature unit: {final_msg}");
        assert!(final_msg.contains("72.4"), "temp not rendered: {final_msg}");
        assert!(
            final_msg.contains("Sacramento, CA"),
            "city not rendered: {final_msg}"
        );
        assert!(
            !final_msg.contains("${"),
            "unresolved placeholder: {final_msg}"
        );
        assert!(!final_msg.contains("{\""), "raw JSON leaked: {final_msg}");
        assert!(!final_msg.contains("null"), "null in output: {final_msg}");
    }
}
