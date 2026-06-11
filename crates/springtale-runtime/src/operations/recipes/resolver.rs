//! Deploy-time derived-input resolution — the universal-recipe engine.
//!
//! A recipe declares [`DerivedInputResolver`]s in its blueprint; this
//! module resolves them into concrete [`RecipeInputs`] values BEFORE
//! placeholder substitution runs. That's what lets a "playbook strategy"
//! take a free target (a city the commander names) and turn it into the
//! exact value the action needs (`latitude=..&longitude=..`), instead of
//! freezing a hardcoded dropdown.
//!
//! Resolution happens ONCE, at deploy — the resolved value is baked into
//! the created rule. Fire-time resolution is deliberately avoided:
//! connector-http does not URL-encode query params, so a free-text city
//! ("Sacramento, CA") would break the request URL; doing it here lets the
//! dependency-free [`encode_query`] percent-encode the city correctly
//! before the geocoding request goes out.

use serde_json::Value;

use super::types::{DerivedInputResolver, Recipe, RecipeInputs};

/// Open-Meteo's keyless geocoding endpoint. No API key, no sign-up.
const GEOCODE_URL: &str = "https://geocoding-api.open-meteo.com/v1/search";

#[derive(Debug, thiserror::Error)]
pub enum ResolverError {
    #[error("the derived input '{0}' has no source value to resolve from")]
    MissingSource(String),
    #[error("I couldn't find a place called '{0}'")]
    NotFound(String),
    #[error("geocoding request failed: {0}")]
    Http(String),
}

/// Resolve every declared derived input into `inputs`, in order. Called
/// by `apply_recipe` between placeholder validation and blueprint
/// application. A failure aborts the deploy before any side effect.
pub async fn apply_derived_inputs(
    recipe: &Recipe,
    inputs: &mut RecipeInputs,
) -> Result<(), ResolverError> {
    for resolver in &recipe.blueprint.derived_inputs {
        match resolver {
            DerivedInputResolver::Geocode {
                source_input_id,
                target_input_id,
            } => {
                let place = inputs
                    .get(source_input_id)
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| ResolverError::MissingSource(source_input_id.clone()))?
                    .to_owned();

                let (lat, lon) = geocode(&place).await?;
                inputs.insert(
                    target_input_id.clone(),
                    Value::String(format!("latitude={lat}&longitude={lon}")),
                );
            }
        }
    }
    Ok(())
}

/// Geocode a free-text place to `(latitude, longitude)` via Open-Meteo.
///
/// Open-Meteo's `name=` matches the BARE place name — "Sacramento", not
/// "Sacramento, CA" (the state lives in `admin1`). So we split the input
/// into a city + optional region, query the city, and disambiguate the
/// results by region (e.g. "Springfield, IL" → the Illinois one). Handles
/// "Sacramento, CA", "London", "Paris, France", "Springfield, IL".
pub async fn geocode(place: &str) -> Result<(f64, f64), ResolverError> {
    let (city, regions) = parse_place_query(place);

    // SECURITY: rustls-only client (safe_http); never a raw reqwest.
    let client = springtale_transport::safe_http::client()
        .map_err(|e| ResolverError::Http(e.to_string()))?;

    let url = format!(
        "{GEOCODE_URL}?name={}&count=10&language=en&format=json",
        encode_query(&city)
    );

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| ResolverError::Http(e.to_string()))?;

    let text = resp
        .text()
        .await
        .map_err(|e| ResolverError::Http(e.to_string()))?;

    let body: Value =
        serde_json::from_str(&text).map_err(|e| ResolverError::Http(e.to_string()))?;

    select_coords(&body, &regions, place)
}

/// Split a free-text place into a bare city (sent to Open-Meteo) and the
/// region terms used to disambiguate. "Sacramento, CA" → ("Sacramento",
/// ["ca"]); "London" → ("London", []); "Paris, France" → ("Paris", ["france"]).
fn parse_place_query(place: &str) -> (String, Vec<String>) {
    let mut parts = place.split(',');
    let city = parts.next().unwrap_or(place).trim().to_owned();
    let regions = parts
        .map(|p| p.trim().to_lowercase())
        .filter(|p| !p.is_empty())
        .collect();
    (city, regions)
}

/// Percent-encode a query-string value (RFC 3986 unreserved set passes
/// through; everything else is `%XX`). Dependency-free — no url crate.
fn encode_query(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Pick `(latitude, longitude)` from an Open-Meteo geocoding response,
/// preferring a result whose region matches (`admin1`/`country`/
/// `country_code`, with US state-abbreviation expansion), else the first
/// (highest-relevance) result. Pure — unit-tested without a live request.
fn select_coords(
    body: &Value,
    regions: &[String],
    place: &str,
) -> Result<(f64, f64), ResolverError> {
    let results = body
        .get("results")
        .and_then(Value::as_array)
        .filter(|a| !a.is_empty())
        .ok_or_else(|| ResolverError::NotFound(place.to_owned()))?;

    let chosen = if regions.is_empty() {
        &results[0]
    } else {
        results
            .iter()
            .find(|r| regions.iter().any(|reg| region_matches(reg, r)))
            .unwrap_or(&results[0])
    };

    let lat = chosen
        .get("latitude")
        .and_then(Value::as_f64)
        .ok_or_else(|| ResolverError::NotFound(place.to_owned()))?;
    let lon = chosen
        .get("longitude")
        .and_then(Value::as_f64)
        .ok_or_else(|| ResolverError::NotFound(place.to_owned()))?;

    Ok((lat, lon))
}

/// Does `region` (lower-cased, e.g. "ca" / "california" / "france") match
/// this geocoding result's `admin1` / `country` / `country_code`?
fn region_matches(region: &str, result: &Value) -> bool {
    let field = |k: &str| result.get(k).and_then(Value::as_str).map(str::to_lowercase);
    let admin1 = field("admin1");
    let candidates = [admin1.clone(), field("country"), field("country_code")];
    if candidates.iter().flatten().any(|c| c == region) {
        return true;
    }
    // US "City, ST": expand the abbreviation and match against admin1.
    if let Some(full) = us_state_full_name(region) {
        return admin1.as_deref() == Some(full);
    }
    false
}

/// US state/territory two-letter abbreviation → lower-cased full name
/// (Open-Meteo's `admin1`). `None` if not a recognized abbreviation.
fn us_state_full_name(abbr: &str) -> Option<&'static str> {
    const STATES: &[(&str, &str)] = &[
        ("al", "alabama"),
        ("ak", "alaska"),
        ("az", "arizona"),
        ("ar", "arkansas"),
        ("ca", "california"),
        ("co", "colorado"),
        ("ct", "connecticut"),
        ("de", "delaware"),
        ("fl", "florida"),
        ("ga", "georgia"),
        ("hi", "hawaii"),
        ("id", "idaho"),
        ("il", "illinois"),
        ("in", "indiana"),
        ("ia", "iowa"),
        ("ks", "kansas"),
        ("ky", "kentucky"),
        ("la", "louisiana"),
        ("me", "maine"),
        ("md", "maryland"),
        ("ma", "massachusetts"),
        ("mi", "michigan"),
        ("mn", "minnesota"),
        ("ms", "mississippi"),
        ("mo", "missouri"),
        ("mt", "montana"),
        ("ne", "nebraska"),
        ("nv", "nevada"),
        ("nh", "new hampshire"),
        ("nj", "new jersey"),
        ("nm", "new mexico"),
        ("ny", "new york"),
        ("nc", "north carolina"),
        ("nd", "north dakota"),
        ("oh", "ohio"),
        ("ok", "oklahoma"),
        ("or", "oregon"),
        ("pa", "pennsylvania"),
        ("ri", "rhode island"),
        ("sc", "south carolina"),
        ("sd", "south dakota"),
        ("tn", "tennessee"),
        ("tx", "texas"),
        ("ut", "utah"),
        ("vt", "vermont"),
        ("va", "virginia"),
        ("wa", "washington"),
        ("wv", "west virginia"),
        ("wi", "wisconsin"),
        ("wy", "wyoming"),
        ("dc", "washington, d.c."),
    ];
    STATES
        .iter()
        .find(|(a, _)| *a == abbr)
        .map(|(_, full)| *full)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::operations::recipes::types::{
        Difficulty, FieldKind, FieldVisibility, InputField, RecipeBlueprint, RecipeCategory,
        RecipeSource,
    };

    #[test]
    fn test_parse_place_query_splits_city_and_region() {
        assert_eq!(
            parse_place_query("Sacramento, CA"),
            ("Sacramento".into(), vec!["ca".into()])
        );
        assert_eq!(parse_place_query("London"), ("London".into(), vec![]));
        assert_eq!(
            parse_place_query("Paris, France"),
            ("Paris".into(), vec!["france".into()])
        );
    }

    /// Real Open-Meteo `name=Sacramento` shape — the city is "Sacramento",
    /// the state is in `admin1`, so the literal "Sacramento, CA" never
    /// matches `name=`. We query the bare city and disambiguate by region.
    fn sacramento_results() -> Value {
        serde_json::json!({
            "results": [
                { "name": "Sacramento", "latitude": 38.58157, "longitude": -121.4944,
                  "country": "United States", "country_code": "US", "admin1": "California" },
                { "name": "Sacramento", "latitude": -23.0, "longitude": -47.0,
                  "country": "Brazil", "country_code": "BR", "admin1": "São Paulo" }
            ]
        })
    }

    #[test]
    fn test_select_coords_disambiguates_by_us_state_abbrev() {
        // "Sacramento, CA" → region ["ca"] → expands to California → picks the US one.
        let (lat, lon) =
            select_coords(&sacramento_results(), &["ca".into()], "Sacramento, CA").unwrap();
        assert!((lat - 38.58157).abs() < 1e-6);
        assert!((lon - -121.4944).abs() < 1e-6);
    }

    #[test]
    fn test_select_coords_no_region_takes_first() {
        let (lat, _) = select_coords(&sacramento_results(), &[], "Sacramento").unwrap();
        assert!((lat - 38.58157).abs() < 1e-6); // most-relevant first result
    }

    #[test]
    fn test_select_coords_disambiguates_springfield() {
        let body = serde_json::json!({
            "results": [
                { "name": "Springfield", "latitude": 37.2, "longitude": -93.3,
                  "admin1": "Missouri", "country_code": "US" },
                { "name": "Springfield", "latitude": 39.8, "longitude": -89.6,
                  "admin1": "Illinois", "country_code": "US" }
            ]
        });
        // "Springfield, IL" → picks Illinois, not the first (Missouri).
        let (lat, _) = select_coords(&body, &["il".into()], "Springfield, IL").unwrap();
        assert!((lat - 39.8).abs() < 1e-6);
    }

    #[test]
    fn test_encode_query_percent_encodes_space_and_comma() {
        assert_eq!(encode_query("Sacramento, CA"), "Sacramento%2C%20CA");
        assert_eq!(encode_query("London"), "London");
        assert_eq!(encode_query("São Paulo"), "S%C3%A3o%20Paulo"); // UTF-8 bytes
    }

    #[test]
    fn test_select_coords_no_results_is_not_found() {
        let body = serde_json::json!({ "generationtime_ms": 0.3 }); // Open-Meteo omits empty `results`
        let err = select_coords(&body, &[], "Zzqxborp").unwrap_err();
        assert!(matches!(err, ResolverError::NotFound(p) if p == "Zzqxborp"));
    }

    /// LIVE: the exact reported input against the real Open-Meteo API.
    /// `cargo test -p springtale-runtime -- --ignored test_geocode_live`.
    #[tokio::test]
    #[ignore = "hits the live Open-Meteo geocoding API"]
    async fn test_geocode_live_sacramento_ca() {
        let (lat, lon) = geocode("Sacramento, CA")
            .await
            .expect("geocode 'Sacramento, CA' should resolve against the live API");
        assert!((lat - 38.58).abs() < 0.5, "lat off: {lat}");
        assert!((lon - (-121.49)).abs() < 0.5, "lon off: {lon}");
    }

    /// LIVE, WHOLE CHAIN: the exact reported user action against the real
    /// Open-Meteo geocoding AND forecast APIs, end to end. Deploy-time
    /// geocode of "Sacramento, CA" → real `${location}` → the recipe's
    /// real forecast URL → live fetch → the recipe's real `Extract`
    /// JSONPaths → the recipe's real `Notify` template → a human sentence
    /// with a live temperature. This is the test that proves the chain
    /// the user walked actually delivers against real data — not a mock.
    /// `cargo test -p springtale-runtime -- --ignored test_weather_chain_live`.
    #[tokio::test]
    #[ignore = "hits the live Open-Meteo geocoding + forecast APIs"]
    async fn test_weather_chain_live_end_to_end() {
        use crate::extraction::extract;
        use crate::operations::recipes::apply::substitute_template_public;
        use springtale_core::rule::action::Action;
        use springtale_core::rule::chain_context::ChainContext;
        use springtale_core::rule::template_resolve::resolve_chain_template;
        use springtale_core::rule::types::Rule;

        // 1. Deploy-time: geocode the free-text city into `${location}`.
        let recipe =
            crate::operations::recipes::builtin::get("weather-briefing").expect("weather recipe");
        let mut inputs = RecipeInputs::empty();
        inputs.insert("city", serde_json::json!("Sacramento, CA"));
        inputs.insert("schedule", serde_json::json!("0 8 * * *"));
        apply_derived_inputs(&recipe, &mut inputs)
            .await
            .expect("live geocode of 'Sacramento, CA'");

        // 2. Substitute → the recipe's real forecast URL with live coords.
        let toml = substitute_template_public(&recipe.blueprint.rules[0].toml, &inputs);
        let rule: Rule = toml::from_str(&toml).expect("rule parses");
        let Action::Chain { steps } = &rule.actions[0] else {
            panic!("expected a Chain action");
        };
        let url = steps
            .iter()
            .find_map(|s| match s {
                Action::RunConnector { params, .. } => params
                    .get("url")
                    .and_then(|v| v.as_str())
                    .map(str::to_owned),
                _ => None,
            })
            .expect("forecast url present");

        // 3. Fire: fetch the REAL Open-Meteo forecast.
        let client = springtale_transport::safe_http::client().expect("http client");
        let body = client
            .get(&url)
            .send()
            .await
            .expect("forecast request")
            .text()
            .await
            .expect("forecast body");

        // 4. Run the recipe's actual Extract + Notify against the live body.
        let mut chain = ChainContext::new(serde_json::Value::Null);
        chain.last_connector_output =
            Some(serde_json::json!({ "status": 200, "headers": {}, "body": body }));
        let mut final_msg = String::new();
        for step in steps {
            match step {
                Action::Extract { kind, .. } => {
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

        // 5. The user sees a real sentence with a live temperature.
        assert!(final_msg.contains("°F"), "no temperature unit: {final_msg}");
        assert!(
            final_msg.contains("Sacramento, CA"),
            "city not rendered: {final_msg}"
        );
        assert!(!final_msg.contains("${"), "placeholder leaked: {final_msg}");
        assert!(!final_msg.contains("{\""), "raw JSON leaked: {final_msg}");
        assert!(!final_msg.contains("null"), "null in output: {final_msg}");
    }

    fn geocode_recipe() -> Recipe {
        Recipe {
            id: "weather-briefing".into(),
            name: "Morning Weather".into(),
            description: "w".into(),
            icon_id: "x".into(),
            category: RecipeCategory::Daily,
            tags: vec![],
            connectors_used: vec![],
            ai_required: false,
            difficulty: Difficulty::Quick,
            source: RecipeSource::Builtin,
            inputs: vec![InputField {
                id: "city".into(),
                label: "City".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: None,
            }],
            blueprint: RecipeBlueprint {
                connector_configs: vec![],
                rules: vec![],
                ai_config: None,
                summary: None,
                derived_inputs: vec![DerivedInputResolver::Geocode {
                    source_input_id: "city".into(),
                    target_input_id: "location".into(),
                }],
            },
        }
    }

    #[tokio::test]
    async fn test_apply_derived_inputs_missing_source_errors() {
        let recipe = geocode_recipe();
        let mut inputs = RecipeInputs::empty(); // no `city`
        let err = apply_derived_inputs(&recipe, &mut inputs)
            .await
            .unwrap_err();
        assert!(matches!(err, ResolverError::MissingSource(s) if s == "city"));
    }
}
