/// Resolve template variables in a string.
///
/// Variables use the `${path.to.field}` syntax. Resolution is against
/// a JSON payload using dot-notation field paths.
///
/// Security: no nested `${}` allowed. No code execution. Variables
/// resolve to string values only. Unresolvable variables are left as-is.
pub fn resolve_template(template: &str, payload: &serde_json::Value) -> String {
    let mut result = String::with_capacity(template.len());
    let mut chars = template.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '$' && chars.peek() == Some(&'{') {
            chars.next(); // consume '{'
            let mut var_name = String::new();
            let mut found_close = false;

            for inner in chars.by_ref() {
                if inner == '}' {
                    found_close = true;
                    break;
                }
                // Reject nested ${ inside a variable (security: no nesting)
                if inner == '$' {
                    // Not a valid variable — emit raw text
                    result.push_str("${");
                    result.push_str(&var_name);
                    result.push('$');
                    var_name.clear();
                    found_close = false;
                    break;
                }
                var_name.push(inner);
            }

            if found_close && !var_name.is_empty() {
                // Resolve the variable against the payload
                match resolve_field(payload, &var_name) {
                    Some(serde_json::Value::String(s)) => result.push_str(s),
                    Some(serde_json::Value::Number(n)) => result.push_str(&n.to_string()),
                    Some(serde_json::Value::Bool(b)) => result.push_str(&b.to_string()),
                    Some(serde_json::Value::Null) => result.push_str("null"),
                    Some(_) => result.push_str(&format!("${{{var_name}}}")), // complex types left as-is
                    None => result.push_str(&format!("${{{var_name}}}")), // unresolvable left as-is
                }
            } else if !found_close {
                // Unclosed ${ — emit raw text
                result.push_str("${");
                result.push_str(&var_name);
            }
        } else {
            result.push(ch);
        }
    }

    result
}

/// Resolve a dotted field path against a JSON value.
fn resolve_field<'a>(payload: &'a serde_json::Value, field: &str) -> Option<&'a serde_json::Value> {
    let mut current = payload;
    for part in field.split('.') {
        match current {
            serde_json::Value::Object(map) => {
                current = map.get(part)?;
            }
            serde_json::Value::Array(arr) => {
                let index: usize = part.parse().ok()?;
                current = arr.get(index)?;
            }
            _ => return None,
        }
    }
    Some(current)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_simple_variable() {
        let payload = json!({"username": "alice"});
        assert_eq!(
            resolve_template("Hello ${username}!", &payload),
            "Hello alice!"
        );
    }

    #[test]
    fn test_nested_path() {
        let payload = json!({"trigger": {"title": "My Stream"}});
        assert_eq!(
            resolve_template("Live: ${trigger.title}", &payload),
            "Live: My Stream"
        );
    }

    #[test]
    fn test_number_value() {
        let payload = json!({"count": 42});
        assert_eq!(resolve_template("Count: ${count}", &payload), "Count: 42");
    }

    #[test]
    fn test_missing_variable_left_as_is() {
        let payload = json!({"a": 1});
        assert_eq!(resolve_template("${missing}", &payload), "${missing}");
    }

    #[test]
    fn test_no_variables() {
        let payload = json!({});
        assert_eq!(resolve_template("plain text", &payload), "plain text");
    }

    #[test]
    fn test_multiple_variables() {
        let payload = json!({"a": "hello", "b": "world"});
        assert_eq!(resolve_template("${a} ${b}", &payload), "hello world");
    }

    #[test]
    fn test_nested_dollar_rejected() {
        let payload = json!({"a": "val"});
        // ${${a}} should not be interpreted as nested
        let result = resolve_template("${${a}}", &payload);
        // The nested $ breaks the first variable — no clean resolution
        assert!(!result.contains("val") || result.contains("$"));
    }

    #[test]
    fn test_unclosed_variable() {
        let payload = json!({"a": "val"});
        assert_eq!(resolve_template("${unclosed", &payload), "${unclosed");
    }

    #[test]
    fn test_bool_value() {
        let payload = json!({"active": true});
        assert_eq!(
            resolve_template("Active: ${active}", &payload),
            "Active: true"
        );
    }
}
