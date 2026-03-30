use regex::RegexBuilder;

use super::condition::Condition;

/// Maximum compiled regex size (prevents ReDoS via counted repetitions).
const REGEX_SIZE_LIMIT: usize = 1_000_000; // 1MB

/// Evaluate a condition tree against a trigger payload.
///
/// This is a pure function with no side effects: `(Condition, Payload) -> bool`.
/// No network. No AI. No I/O.
pub fn evaluate_condition(condition: &Condition, payload: &serde_json::Value) -> bool {
    match condition {
        Condition::And { conditions } => conditions.iter().all(|c| evaluate_condition(c, payload)),

        Condition::Or { conditions } => conditions.iter().any(|c| evaluate_condition(c, payload)),

        Condition::Not { condition } => !evaluate_condition(condition, payload),

        Condition::FieldEquals { field, value } => {
            resolve_field(payload, field).is_some_and(|v| v == value)
        }

        Condition::Contains { field, value } => resolve_field(payload, field)
            .and_then(|v| v.as_str())
            .is_some_and(|s| s.contains(value.as_str())),

        Condition::Regex { field, pattern } => {
            let Some(field_value) =
                resolve_field(payload, field).and_then(|v| v.as_str().map(String::from))
            else {
                return false;
            };
            RegexBuilder::new(pattern)
                .size_limit(REGEX_SIZE_LIMIT)
                .build()
                .is_ok_and(|re| re.is_match(&field_value))
        }

        Condition::TimeInRange { start, end } => {
            let now = chrono::Local::now().format("%H:%M").to_string();
            // Simple string comparison works for HH:MM format
            if start <= end {
                now >= *start && now <= *end
            } else {
                // Wraps midnight (e.g., "22:00" - "06:00")
                now >= *start || now <= *end
            }
        }

        Condition::DayOfWeek { days } => {
            let today = chrono::Local::now()
                .format("%w")
                .to_string()
                .parse::<u8>()
                .unwrap_or(0);
            days.contains(&today)
        }
    }
}

/// Resolve a dotted field path against a JSON value.
///
/// `"trigger.category"` resolves to `payload["trigger"]["category"]`.
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
    fn test_field_equals_match() {
        let payload = json!({"category": "gaming"});
        let cond = Condition::FieldEquals {
            field: "category".into(),
            value: json!("gaming"),
        };
        assert!(evaluate_condition(&cond, &payload));
    }

    #[test]
    fn test_field_equals_no_match() {
        let payload = json!({"category": "music"});
        let cond = Condition::FieldEquals {
            field: "category".into(),
            value: json!("gaming"),
        };
        assert!(!evaluate_condition(&cond, &payload));
    }

    #[test]
    fn test_field_equals_missing_field() {
        let payload = json!({"other": "value"});
        let cond = Condition::FieldEquals {
            field: "category".into(),
            value: json!("gaming"),
        };
        assert!(!evaluate_condition(&cond, &payload));
    }

    #[test]
    fn test_nested_field_resolution() {
        let payload = json!({"trigger": {"category": "gaming"}});
        let cond = Condition::FieldEquals {
            field: "trigger.category".into(),
            value: json!("gaming"),
        };
        assert!(evaluate_condition(&cond, &payload));
    }

    #[test]
    fn test_contains_match() {
        let payload = json!({"title": "Playing Halo Infinite"});
        let cond = Condition::Contains {
            field: "title".into(),
            value: "Halo".into(),
        };
        assert!(evaluate_condition(&cond, &payload));
    }

    #[test]
    fn test_contains_no_match() {
        let payload = json!({"title": "Playing Halo Infinite"});
        let cond = Condition::Contains {
            field: "title".into(),
            value: "Zelda".into(),
        };
        assert!(!evaluate_condition(&cond, &payload));
    }

    #[test]
    fn test_regex_match() {
        let payload = json!({"filename": "report_2026.pdf"});
        let cond = Condition::Regex {
            field: "filename".into(),
            pattern: r"\.(pdf|docx)$".into(),
        };
        assert!(evaluate_condition(&cond, &payload));
    }

    #[test]
    fn test_regex_no_match() {
        let payload = json!({"filename": "image.png"});
        let cond = Condition::Regex {
            field: "filename".into(),
            pattern: r"\.(pdf|docx)$".into(),
        };
        assert!(!evaluate_condition(&cond, &payload));
    }

    #[test]
    fn test_regex_invalid_pattern() {
        let payload = json!({"filename": "test"});
        let cond = Condition::Regex {
            field: "filename".into(),
            pattern: r"[invalid".into(),
        };
        // Invalid regex → evaluates to false, does not panic
        assert!(!evaluate_condition(&cond, &payload));
    }

    #[test]
    fn test_and_all_true() {
        let payload = json!({"a": 1, "b": 2});
        let cond = Condition::And {
            conditions: vec![
                Condition::FieldEquals {
                    field: "a".into(),
                    value: json!(1),
                },
                Condition::FieldEquals {
                    field: "b".into(),
                    value: json!(2),
                },
            ],
        };
        assert!(evaluate_condition(&cond, &payload));
    }

    #[test]
    fn test_and_one_false() {
        let payload = json!({"a": 1, "b": 2});
        let cond = Condition::And {
            conditions: vec![
                Condition::FieldEquals {
                    field: "a".into(),
                    value: json!(1),
                },
                Condition::FieldEquals {
                    field: "b".into(),
                    value: json!(99),
                },
            ],
        };
        assert!(!evaluate_condition(&cond, &payload));
    }

    #[test]
    fn test_or_one_true() {
        let payload = json!({"a": 1});
        let cond = Condition::Or {
            conditions: vec![
                Condition::FieldEquals {
                    field: "a".into(),
                    value: json!(99),
                },
                Condition::FieldEquals {
                    field: "a".into(),
                    value: json!(1),
                },
            ],
        };
        assert!(evaluate_condition(&cond, &payload));
    }

    #[test]
    fn test_not_inverts() {
        let payload = json!({"a": 1});
        let cond = Condition::Not {
            condition: Box::new(Condition::FieldEquals {
                field: "a".into(),
                value: json!(1),
            }),
        };
        assert!(!evaluate_condition(&cond, &payload));
    }

    #[test]
    fn test_regex_large_pattern_does_not_panic() {
        // A counted repetition pattern that expands beyond size_limit
        // should evaluate to false, not panic or hang
        let payload = json!({"data": "test"});
        let cond = Condition::Regex {
            field: "data".into(),
            pattern: format!("a{{{}}}", 100_000_000), // a{100000000} — enormous expansion
        };
        // Should return false (regex fails to compile due to size_limit)
        assert!(!evaluate_condition(&cond, &payload));
    }

    #[test]
    fn test_array_index_resolution() {
        let payload = json!({"items": ["a", "b", "c"]});
        let cond = Condition::FieldEquals {
            field: "items.1".into(),
            value: json!("b"),
        };
        assert!(evaluate_condition(&cond, &payload));
    }
}
