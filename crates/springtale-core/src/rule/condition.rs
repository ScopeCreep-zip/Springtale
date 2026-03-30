use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Maximum nesting depth for condition trees.
pub const MAX_CONDITION_DEPTH: u32 = 8;

/// A condition that must be met for a rule's actions to execute.
///
/// Conditions form a tree: `And`, `Or`, `Not` compose leaf conditions.
/// The tree depth is limited to `MAX_CONDITION_DEPTH` to prevent DoS
/// via deeply nested conditions.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type")]
pub enum Condition {
    /// All child conditions must be true.
    And { conditions: Vec<Condition> },

    /// At least one child condition must be true.
    Or { conditions: Vec<Condition> },

    /// Inverts the child condition.
    Not { condition: Box<Condition> },

    /// A field in the trigger payload equals a specific value.
    FieldEquals {
        field: String,
        value: serde_json::Value,
    },

    /// A field in the trigger payload contains a substring.
    Contains { field: String, value: String },

    /// A field in the trigger payload matches a regex.
    Regex { field: String, pattern: String },

    /// The current time is within a range (24h format, e.g., "09:00"-"17:00").
    TimeInRange { start: String, end: String },

    /// The current day matches (0=Sunday, 6=Saturday).
    DayOfWeek { days: Vec<u8> },
}

impl Condition {
    /// Calculate the depth of this condition tree.
    pub fn depth(&self) -> u32 {
        match self {
            Condition::And { conditions } | Condition::Or { conditions } => {
                1 + conditions.iter().map(|c| c.depth()).max().unwrap_or(0)
            }
            Condition::Not { condition } => 1 + condition.depth(),
            _ => 1,
        }
    }

    /// Validate that this condition tree does not exceed the max depth.
    pub fn validate_depth(&self) -> Result<(), crate::error::CoreError> {
        let depth = self.depth();
        if depth > MAX_CONDITION_DEPTH {
            return Err(crate::error::CoreError::ConditionEval(format!(
                "condition depth {depth} exceeds maximum {MAX_CONDITION_DEPTH}"
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_leaf_depth_is_one() {
        let c = Condition::FieldEquals {
            field: "x".into(),
            value: serde_json::json!(1),
        };
        assert_eq!(c.depth(), 1);
    }

    #[test]
    fn test_nested_depth() {
        let c = Condition::And {
            conditions: vec![Condition::Or {
                conditions: vec![Condition::FieldEquals {
                    field: "x".into(),
                    value: serde_json::json!(1),
                }],
            }],
        };
        assert_eq!(c.depth(), 3);
    }

    #[test]
    fn test_max_depth_exceeded() {
        // Build a chain of 9 nested Not conditions (depth 9 > max 8)
        let mut c = Condition::FieldEquals {
            field: "x".into(),
            value: serde_json::json!(1),
        };
        for _ in 0..9 {
            c = Condition::Not {
                condition: Box::new(c),
            };
        }
        assert!(c.validate_depth().is_err());
    }

    #[test]
    fn test_max_depth_at_limit() {
        // Build exactly 8 levels deep (should pass)
        let mut c = Condition::FieldEquals {
            field: "x".into(),
            value: serde_json::json!(1),
        };
        for _ in 0..7 {
            c = Condition::Not {
                condition: Box::new(c),
            };
        }
        assert_eq!(c.depth(), 8);
        assert!(c.validate_depth().is_ok());
    }
}
