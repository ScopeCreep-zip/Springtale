//! Lightweight JSON Schema validation for structured-extraction
//! responses.
//!
//! Constrained-decoding adapters (OpenAI strict, Ollama
//! `format: schema`) are grammar-correct by construction. The
//! forced-tool-use fallback (older Anthropic models) and any
//! OpenAI-compatible endpoint claiming `strict: true` without
//! actually constraining decoding can still emit shape-broken
//! output. This module is the tripwire that catches those cases
//! and routes them through
//! [`super::error::ExtractorError::OutputInvalid`].
//!
//! Scope: validates the subset of JSON Schema that recipe authors
//! actually use — `type`, `properties`, `required`, `items`,
//! `enum`. Anything more elaborate falls through as "shape ok"
//! since the adapter's grammar constraint is the real
//! enforcement mechanism. Keeps the dependency footprint zero —
//! we deliberately don't pull `jsonschema` here.

/// Validate `value` against the JSON-Schema subset described in
/// the module docs. Returns `Err` with a human-readable reason on
/// mismatch, `Ok(())` on success.
pub fn validate_against(
    value: &serde_json::Value,
    schema: &serde_json::Value,
) -> Result<(), String> {
    let schema_obj = match schema.as_object() {
        Some(o) => o,
        None => return Ok(()), // No object schema — nothing to check.
    };

    if let Some(ty) = schema_obj.get("type").and_then(|t| t.as_str()) {
        check_type(value, ty)?;
    }

    if let (Some(props), Some(obj)) = (
        schema_obj.get("properties").and_then(|p| p.as_object()),
        value.as_object(),
    ) {
        for (field, sub_schema) in props {
            if let Some(sub_value) = obj.get(field) {
                if let Err(reason) = validate_against(sub_value, sub_schema) {
                    return Err(format!("field `{field}`: {reason}"));
                }
            }
        }
    }

    if let (Some(required), Some(obj)) = (
        schema_obj.get("required").and_then(|r| r.as_array()),
        value.as_object(),
    ) {
        for entry in required {
            if let Some(field) = entry.as_str() {
                if !obj.contains_key(field) {
                    return Err(format!("missing required field `{field}`"));
                }
            }
        }
    }

    if let (Some(items_schema), Some(arr)) =
        (schema_obj.get("items"), value.as_array())
    {
        for (i, item) in arr.iter().enumerate() {
            if let Err(reason) = validate_against(item, items_schema) {
                return Err(format!("item[{i}]: {reason}"));
            }
        }
    }

    if let Some(enum_vals) = schema_obj.get("enum").and_then(|e| e.as_array()) {
        if !enum_vals.iter().any(|v| v == value) {
            return Err("value not in declared enum".into());
        }
    }

    Ok(())
}

fn check_type(value: &serde_json::Value, ty: &str) -> Result<(), String> {
    let matches = match ty {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "integer" => value.is_i64() || value.is_u64(),
        "number" => value.is_number(),
        "boolean" => value.is_boolean(),
        "null" => value.is_null(),
        _ => true, // Unknown type — let the adapter's grammar enforce.
    };
    if matches {
        Ok(())
    } else {
        Err(format!("expected type `{ty}`, got {}", json_type_name(value)))
    }
}

fn json_type_name(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "boolean",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn object_with_required_field_passes() {
        let schema = json!({
            "type": "object",
            "properties": { "title": { "type": "string" } },
            "required": ["title"]
        });
        let value = json!({ "title": "hello" });
        assert!(validate_against(&value, &schema).is_ok());
    }

    #[test]
    fn missing_required_field_fails() {
        let schema = json!({
            "type": "object",
            "required": ["title"]
        });
        let value = json!({ "other": "x" });
        let err = validate_against(&value, &schema).unwrap_err();
        assert!(err.contains("missing required field"));
    }

    #[test]
    fn wrong_type_fails() {
        let schema = json!({ "type": "string" });
        let value = json!(42);
        let err = validate_against(&value, &schema).unwrap_err();
        assert!(err.contains("expected type"));
    }

    #[test]
    fn array_items_validated() {
        let schema = json!({
            "type": "array",
            "items": { "type": "integer" }
        });
        assert!(validate_against(&json!([1, 2, 3]), &schema).is_ok());
        assert!(validate_against(&json!([1, "two"]), &schema).is_err());
    }

    #[test]
    fn enum_constraint_enforced() {
        let schema = json!({ "enum": ["draft", "published"] });
        assert!(validate_against(&json!("draft"), &schema).is_ok());
        assert!(validate_against(&json!("unknown"), &schema).is_err());
    }
}
