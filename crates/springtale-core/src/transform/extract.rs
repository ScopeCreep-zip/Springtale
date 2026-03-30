/// Extract a field from a JSON payload using dot-notation.
///
/// Returns `None` if the path doesn't resolve.
pub fn extract_field(payload: &serde_json::Value, path: &str) -> Option<serde_json::Value> {
    let mut current = payload;
    for part in path.split('.') {
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
    Some(current.clone())
}

/// Extract multiple fields into a new JSON object.
pub fn extract_fields(payload: &serde_json::Value, paths: &[&str]) -> serde_json::Value {
    let mut result = serde_json::Map::new();
    for path in paths {
        if let Some(value) = extract_field(payload, path) {
            // Use the last segment of the path as the key
            let key = path.rsplit('.').next().unwrap_or(path);
            result.insert(key.to_owned(), value);
        }
    }
    serde_json::Value::Object(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_extract_simple() {
        let payload = json!({"name": "alice"});
        assert_eq!(extract_field(&payload, "name"), Some(json!("alice")));
    }

    #[test]
    fn test_extract_nested() {
        let payload = json!({"user": {"name": "alice"}});
        assert_eq!(extract_field(&payload, "user.name"), Some(json!("alice")));
    }

    #[test]
    fn test_extract_missing() {
        let payload = json!({"a": 1});
        assert_eq!(extract_field(&payload, "b"), None);
    }

    #[test]
    fn test_extract_multiple() {
        let payload = json!({"user": {"name": "alice", "age": 30}, "status": "active"});
        let result = extract_fields(&payload, &["user.name", "status"]);
        assert_eq!(result, json!({"name": "alice", "status": "active"}));
    }
}
