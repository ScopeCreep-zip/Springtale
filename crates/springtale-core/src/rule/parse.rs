use super::types::Rule;
use crate::error::CoreError;

/// Parse a TOML string into a Rule.
///
/// Validates condition depth and chain depth after parsing.
pub fn parse_rule(toml_str: &str) -> Result<Rule, CoreError> {
    let rule: Rule = toml::from_str(toml_str).map_err(|e| CoreError::RuleParse(e.to_string()))?;

    // Validate condition depth
    for condition in &rule.conditions {
        condition.validate_depth()?;
    }

    // Validate chain depth in actions
    validate_chain_depth(&rule.actions, 0)?;

    Ok(rule)
}

/// Recursively validate that Chain actions don't exceed max depth.
fn validate_chain_depth(
    actions: &[super::action::Action],
    current_depth: u32,
) -> Result<(), CoreError> {
    use super::action::{Action, MAX_CHAIN_DEPTH};

    for action in actions {
        if let Action::Chain { steps } = action {
            let new_depth = current_depth + 1;
            if new_depth > MAX_CHAIN_DEPTH {
                return Err(CoreError::RuleParse(format!(
                    "chain depth {new_depth} exceeds maximum {MAX_CHAIN_DEPTH}"
                )));
            }
            validate_chain_depth(steps, new_depth)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_rule() {
        let toml = r#"
            name = "test-rule"
            description = "A test rule"

            [trigger]
            type = "Cron"
            expression = "0 9 * * *"

            [[actions]]
            type = "SendMessage"
            text = "Good morning!"
        "#;

        let rule = parse_rule(toml);
        assert!(rule.is_ok(), "parse failed: {:?}", rule.err());
        let rule = rule.ok();
        assert_eq!(rule.as_ref().map(|r| r.name.as_str()), Some("test-rule"));
    }

    #[test]
    fn test_parse_rule_with_conditions() {
        let toml = r#"
            name = "conditional-rule"

            [trigger]
            type = "ConnectorEvent"
            connector = "connector-kick"
            event = "stream_live"

            [[conditions]]
            type = "FieldEquals"
            field = "category"
            value = "gaming"

            [[actions]]
            type = "RunConnector"
            connector = "connector-bluesky"
            action = "create_post"

            [actions.params]
            text = "Stream is live!"
        "#;

        let rule = parse_rule(toml);
        assert!(rule.is_ok(), "parse failed: {:?}", rule.err());
    }

    #[test]
    fn test_parse_chain_action() {
        let toml = r#"
            name = "chain-rule"

            [trigger]
            type = "Cron"
            expression = "0 7 * * *"

            [[actions]]
            type = "Chain"

            [[actions.steps]]
            type = "RunConnector"
            connector = "connector-http"
            action = "get"

            [[actions.steps]]
            type = "SendMessage"
            text = "Done"
        "#;

        let rule = parse_rule(toml);
        assert!(rule.is_ok(), "parse failed: {:?}", rule.err());
    }

    #[test]
    fn test_parse_invalid_toml() {
        let toml = "this is not valid toml {{{";
        let result = parse_rule(toml);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_rejects_deep_chain() {
        // Chain depth 5 exceeds MAX_CHAIN_DEPTH (4)
        let toml = r#"
            name = "deep-chain"

            [trigger]
            type = "Cron"
            expression = "0 * * * *"

            [[actions]]
            type = "Chain"

            [[actions.steps]]
            type = "Chain"

            [[actions.steps.steps]]
            type = "Chain"

            [[actions.steps.steps.steps]]
            type = "Chain"

            [[actions.steps.steps.steps.steps]]
            type = "Chain"

            [[actions.steps.steps.steps.steps.steps]]
            type = "SendMessage"
            text = "too deep"
        "#;

        let result = parse_rule(toml);
        assert!(result.is_err(), "should reject chain depth > 4");
    }

    #[test]
    fn test_parse_rejects_deep_conditions() {
        // 9 levels of nesting exceeds MAX_CONDITION_DEPTH (8)
        let toml = r#"
            name = "deep-condition"

            [trigger]
            type = "Cron"
            expression = "0 * * * *"

            [[conditions]]
            type = "Not"
            [conditions.condition]
            type = "Not"
            [conditions.condition.condition]
            type = "Not"
            [conditions.condition.condition.condition]
            type = "Not"
            [conditions.condition.condition.condition.condition]
            type = "Not"
            [conditions.condition.condition.condition.condition.condition]
            type = "Not"
            [conditions.condition.condition.condition.condition.condition.condition]
            type = "Not"
            [conditions.condition.condition.condition.condition.condition.condition.condition]
            type = "Not"
            [conditions.condition.condition.condition.condition.condition.condition.condition.condition]
            type = "FieldEquals"
            field = "x"
            value = 1

            [[actions]]
            type = "SendMessage"
            text = "too deep"
        "#;

        let result = parse_rule(toml);
        assert!(result.is_err(), "should reject condition depth > 8");
    }
}
