//! JSONPath (RFC 9535) extraction via `serde_json_path`.
//!
//! Authors declare `{ field_name: "$.json.path" }`. Each path
//! resolves with original JSON types preserved (numbers stay
//! numbers, arrays stay arrays). Paths that match multiple values
//! return the first match — recipe authors use a `[*]` suffix to
//! get arrays explicitly.
//!
//! ## Source shape
//!
//! Upstream sources reach the extractor as either an already-parsed
//! `Value` (e.g. another `Extract` step's output) or as a `Value::String`
//! holding raw JSON text (e.g. `connector-http.get`'s `body` field,
//! which is the response body verbatim). JSONPath traversal only
//! works against parsed JSON, so a string source is parsed once
//! up-front. Strings that fail to parse surface as a clear
//! `ExtractError::JsonPath` — much better than silently returning
//! null and leaving the recipe's downstream `${last_extract_output.X}`
//! placeholders unresolved.

use serde_json::{Map, Value};
use serde_json_path::JsonPath;

use super::ExtractError;

pub fn extract(source: &Value, schema: &Map<String, Value>) -> Result<Value, ExtractError> {
    // Auto-parse a JSON-encoded string source so the recipe author
    // writes the JSONPath against the *data shape*, not against the
    // raw response container the connector handed us.
    let parsed_owned: Option<Value> = match source {
        Value::String(s) => {
            let trimmed = s.trim_start();
            if trimmed.starts_with('{') || trimmed.starts_with('[') {
                Some(serde_json::from_str(s).map_err(|e| ExtractError::JsonPath {
                    path: "(source body)".to_owned(),
                    reason: format!("upstream source is a string but did not parse as JSON: {e}"),
                })?)
            } else {
                None
            }
        }
        _ => None,
    };
    let effective_source: &Value = parsed_owned.as_ref().unwrap_or(source);

    let mut out = Map::with_capacity(schema.len());
    for (field, raw_path) in schema {
        let path_str = raw_path
            .as_str()
            .ok_or_else(|| ExtractError::SchemaFieldType {
                field: field.clone(),
            })?;
        let path = JsonPath::parse(path_str).map_err(|e| ExtractError::JsonPath {
            path: path_str.to_owned(),
            reason: e.to_string(),
        })?;

        let matches: Vec<&Value> = path.query(effective_source).all();
        let value = match matches.len() {
            0 => Value::Null,
            1 => matches[0].clone(),
            _ => Value::Array(matches.into_iter().cloned().collect()),
        };
        out.insert(field.clone(), value);
    }
    Ok(Value::Object(out))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    fn weather_sample() -> Value {
        // Mirrors wttr.in's `?format=j1` shape.
        json!({
            "current_condition": [{
                "temp_C": "18",
                "weatherDesc": [{ "value": "Partly cloudy" }],
                "humidity": "57"
            }]
        })
    }

    #[test]
    fn extracts_single_path_with_type_preservation() {
        let schema: Map<String, Value> = serde_json::from_value(json!({
            "temp": "$.current_condition[0].temp_C",
            "desc": "$.current_condition[0].weatherDesc[0].value",
        }))
        .unwrap();
        let out = extract(&weather_sample(), &schema).unwrap();
        assert_eq!(out["temp"], "18");
        assert_eq!(out["desc"], "Partly cloudy");
    }

    #[test]
    fn extracts_multiple_matches_as_array() {
        let source = json!({
            "items": [
                { "name": "a" },
                { "name": "b" },
                { "name": "c" }
            ]
        });
        let schema: Map<String, Value> = serde_json::from_value(json!({
            "names": "$.items[*].name",
        }))
        .unwrap();
        let out = extract(&source, &schema).unwrap();
        assert_eq!(out["names"][0], "a");
        assert_eq!(out["names"][1], "b");
        assert_eq!(out["names"][2], "c");
    }

    #[test]
    fn missing_path_returns_null() {
        let schema: Map<String, Value> = serde_json::from_value(json!({
            "missing": "$.nope",
        }))
        .unwrap();
        let out = extract(&weather_sample(), &schema).unwrap();
        assert_eq!(out["missing"], Value::Null);
    }

    #[test]
    fn invalid_jsonpath_returns_path_error() {
        let schema: Map<String, Value> = serde_json::from_value(json!({
            "bad": "$.[invalid",
        }))
        .unwrap();
        let err = extract(&weather_sample(), &schema).unwrap_err();
        assert!(matches!(err, ExtractError::JsonPath { .. }));
    }

    #[test]
    fn parses_json_encoded_string_source() {
        // Real-world shape: `connector-http.get` returns
        // `output.body` as a raw response-text string. The recipe's
        // `Extract { source: "last_connector_output.body" }` resolves
        // to that string. JSONPath must query the parsed JSON, not
        // the literal string.
        let raw_body = serde_json::to_string(&weather_sample()).unwrap();
        let source = Value::String(raw_body);
        let schema: Map<String, Value> = serde_json::from_value(json!({
            "value": "$.current_condition[0].weatherDesc[0].value",
        }))
        .unwrap();
        let out = extract(&source, &schema).unwrap();
        assert_eq!(out["value"], "Partly cloudy");
    }

    #[test]
    fn non_json_string_source_surfaces_clear_error() {
        // A string that doesn't start with `{` / `[` is treated as a
        // plain string and queried as-is (returns null for any path
        // that asks for object/array members). A string that LOOKS
        // like JSON but is malformed surfaces a JsonPath parse error.
        let source = Value::String("{ not actually json".to_owned());
        let schema: Map<String, Value> = serde_json::from_value(json!({
            "value": "$.foo",
        }))
        .unwrap();
        let err = extract(&source, &schema).unwrap_err();
        assert!(matches!(err, ExtractError::JsonPath { .. }));
    }
}
