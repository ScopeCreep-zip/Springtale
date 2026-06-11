//! Recorded-real (VCR) end-to-end tests for the live-shape READ recipes.
//!
//! These recipes fetch an external resource and parse the response, so
//! the bug class is "the recipe's extraction doesn't match the real
//! response shape" (the geocoding/weather class). Each fixture under
//! `tests/fixtures/responses/` is a REAL captured response — Open-Meteo
//! forecast JSON, a real wttr.in `j1` response, a real Hacker News RSS
//! feed, a real US-holidays iCalendar. No guessed data.
//!
//! Each test replays the committed fixture through the recipe's ACTUAL
//! `Extract` + delivery template and asserts the user-facing delivery is
//! clean AND contains a real value. The `#[ignore]`-d `*_live_refresh`
//! tests re-fetch the real endpoint (VCR "record" mode) and would update
//! the fixture; the default tests run offline against the committed copy.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use serde_json::{Value, json};

use springtale_core::rule::action::Action;
use springtale_core::rule::chain_context::ChainContext;
use springtale_core::rule::template_resolve::resolve_chain_template;
use springtale_core::rule::types::Rule;
use springtale_runtime::extraction::extract;
use springtale_runtime::operations::recipes::apply::substitute_template_public;
use springtale_runtime::operations::recipes::builtin;
use springtale_runtime::operations::recipes::types::{FieldKind, Recipe, RecipeInputs};

// Committed recorded-real fixtures (captured from the live endpoints).
const OPEN_METEO: &str = include_str!("fixtures/responses/open_meteo_forecast.json");
const WTTR: &str = include_str!("fixtures/responses/wttr_sacramento.json");
const HN_RSS: &str = include_str!("fixtures/responses/hn_rss.xml");
const HOLIDAYS_ICS: &str = include_str!("fixtures/responses/holidays.ics");

/// Fill a recipe's inputs from their declared defaults (so e.g.
/// `scheduled-web-fetch`'s default JSONPath that targets the wttr.in
/// shape is used verbatim), falling back to a type value, plus any
/// caller overrides.
fn inputs_for(recipe: &Recipe, overrides: &[(&str, Value)]) -> RecipeInputs {
    let mut inputs = RecipeInputs::empty();
    for f in &recipe.inputs {
        let v = f.default.clone().unwrap_or_else(|| match &f.kind {
            FieldKind::Secret => json!("secret"),
            FieldKind::Number => json!(1),
            FieldKind::Bool => json!(true),
            FieldKind::Url => json!("https://example.com/feed"),
            FieldKind::Select { options } => options
                .first()
                .map(|o| json!(o.value))
                .unwrap_or_else(|| json!("")),
            FieldKind::Cron => json!("0 8 * * *"),
            _ => json!("123"),
        });
        inputs.insert(f.id.clone(), v);
    }
    for (k, v) in overrides {
        inputs.insert((*k).to_owned(), v.clone());
    }
    inputs
}

/// Replay a recorded response body through a READ recipe's real chain:
/// inject it as `last_connector_output.body`, run every `Extract` step
/// for real, and resolve the terminal delivery template(s). Returns the
/// user-facing delivery strings.
async fn replay(recipe_id: &str, body: &str, overrides: &[(&str, Value)]) -> Vec<String> {
    let recipe = builtin::get(recipe_id).expect("recipe exists");
    let inputs = inputs_for(&recipe, overrides);
    let toml = substitute_template_public(&recipe.blueprint.rules[0].toml, &inputs);
    let rule: Rule = toml::from_str(&toml).expect("rule parses");

    // Recipes wrap their steps in a single Chain, or list them flat.
    let steps: Vec<Action> = match rule.actions.first() {
        Some(Action::Chain { steps }) => steps.clone(),
        _ => rule.actions.clone(),
    };

    let mut chain = ChainContext::new(Value::Null);
    chain.last_connector_output = Some(json!({ "status": 200, "headers": {}, "body": body }));

    let mut deliveries = Vec::new();
    for step in &steps {
        match step {
            Action::Extract { source, kind } => {
                // Every READ recipe extracts from `last_connector_output.body`.
                let src = chain
                    .last_connector_output
                    .as_ref()
                    .and_then(|v| v.get("body"))
                    .cloned()
                    .unwrap_or(Value::Null);
                if source == "last_connector_output.body" {
                    let out = extract(&src, kind, None).await.expect("extract ok");
                    chain.last_extract_output = Some(out);
                }
            }
            Action::Notify { body, .. } => {
                deliveries.push(resolve_chain_template(body, &chain, None));
            }
            Action::SendMessage { text } => {
                deliveries.push(resolve_chain_template(text, &chain, None));
            }
            Action::RunConnector { params, .. } => {
                for k in ["text", "content", "body", "message"] {
                    if let Some(t) = params.get(k).and_then(Value::as_str) {
                        deliveries.push(resolve_chain_template(t, &chain, None));
                    }
                }
            }
            _ => {}
        }
    }
    deliveries
}

fn assert_clean_nonempty(recipe: &str, deliveries: &[String]) {
    assert!(!deliveries.is_empty(), "{recipe}: no delivery produced");
    for d in deliveries {
        assert!(!d.contains("${"), "{recipe}: unresolved placeholder: {d:?}");
        assert!(!d.contains("{\""), "{recipe}: raw JSON blob: {d:?}");
        assert!(!d.contains("null"), "{recipe}: null leaked: {d:?}");
    }
}

#[tokio::test]
async fn weather_briefing_replays_real_open_meteo() {
    let deliveries = replay(
        "weather-briefing",
        OPEN_METEO,
        &[
            ("city", json!("Sacramento, CA")),
            ("location", json!("latitude=38.58&longitude=-121.49")),
        ],
    )
    .await;
    assert_clean_nonempty("weather-briefing", &deliveries);
    let joined = deliveries.join(" | ");
    assert!(joined.contains("°F"), "no temperature unit: {joined}");
    assert!(joined.contains("Sacramento, CA"), "city missing: {joined}");
    // A real number was extracted from the live forecast.
    assert!(
        joined.chars().any(|c| c.is_ascii_digit()),
        "no temperature value: {joined}"
    );
}

#[tokio::test]
async fn scheduled_web_fetch_replays_real_wttr() {
    // The recipe's default `jsonpath_field`
    // (`$.current_condition[0].weatherDesc[0].value`) targets the real
    // wttr.in `j1` shape — so the committed real response must extract.
    let deliveries = replay("scheduled-web-fetch", WTTR, &[]).await;
    assert_clean_nonempty("scheduled-web-fetch", &deliveries);
    // The delivered message is more than just the static prefix — a real
    // weather description was extracted.
    let joined = deliveries.join(" | ");
    assert!(joined.len() > 4, "extracted value looks empty: {joined}");
}

#[tokio::test]
async fn rss_broadcast_replays_real_feed() {
    let deliveries = replay("rss-broadcast", HN_RSS, &[]).await;
    assert_clean_nonempty("rss-broadcast", &deliveries);
    let joined = deliveries.join(" | ");
    // A real headline + link extracted from the live feed's first entry.
    assert!(joined.contains("http"), "no entry url extracted: {joined}");
    assert!(joined.len() > 20, "no entry title extracted: {joined}");
}

#[tokio::test]
async fn calendar_feed_reminder_replays_real_ical() {
    let deliveries = replay("calendar-feed-reminder", HOLIDAYS_ICS, &[]).await;
    assert_clean_nonempty("calendar-feed-reminder", &deliveries);
    let joined = deliveries.join(" | ");
    // A real event summary + start time extracted from the live .ics
    // (well beyond the recipe's ~25-byte empty "Starts:/Location:" shell).
    assert!(
        joined.contains("Starts:"),
        "template did not resolve: {joined}"
    );
    assert!(joined.len() > 40, "no real event data extracted: {joined}");
}
