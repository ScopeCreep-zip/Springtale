/// Filter an array in a JSON payload by a predicate field value.
pub fn filter_array(
    array: &[serde_json::Value],
    field: &str,
    value: &serde_json::Value,
) -> Vec<serde_json::Value> {
    array
        .iter()
        .filter(|item| item.get(field).is_some_and(|v| v == value))
        .cloned()
        .collect()
}

/// Map: extract a single field from each element of an array.
pub fn map_field(array: &[serde_json::Value], field: &str) -> Vec<serde_json::Value> {
    array
        .iter()
        .filter_map(|item| item.get(field).cloned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_filter_array() {
        let items = vec![
            json!({"name": "a", "type": "x"}),
            json!({"name": "b", "type": "y"}),
            json!({"name": "c", "type": "x"}),
        ];
        let result = filter_array(&items, "type", &json!("x"));
        assert_eq!(result.len(), 2);
        assert_eq!(result[0]["name"], "a");
        assert_eq!(result[1]["name"], "c");
    }

    #[test]
    fn test_map_field() {
        let items = vec![
            json!({"name": "a", "score": 10}),
            json!({"name": "b", "score": 20}),
        ];
        let result = map_field(&items, "name");
        assert_eq!(result, vec![json!("a"), json!("b")]);
    }
}
