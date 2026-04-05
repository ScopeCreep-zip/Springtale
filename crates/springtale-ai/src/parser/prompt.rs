use crate::adapter::ConnectorInfo;

/// Build the system prompt for NL→Rule generation.
///
/// Includes the Rule TOML schema description and available connector
/// metadata (filtered by each connector's disclosure level).
pub fn build_system_prompt(connectors: &[ConnectorInfo]) -> String {
    let mut prompt = String::from(
        "You are a rule generator for the Springtale automation platform.\n\
         Your task is to convert natural language intents into TOML rules.\n\n\
         A rule has this structure:\n\
         ```toml\n\
         [rule]\n\
         name = \"rule-name\"\n\
         enabled = false\n\n\
         [trigger]\n\
         type = \"ConnectorEvent\" | \"Cron\" | \"FileWatch\" | \"Webhook\"\n\
         # For ConnectorEvent:\n\
         connector = \"connector-name\"\n\
         event = \"event-name\"\n\
         # For Cron:\n\
         expression = \"0 9 * * *\"\n\n\
         [[conditions]]\n\
         type = \"FieldEquals\" | \"Contains\" | \"Regex\"\n\
         field = \"field.path\"\n\
         value = \"expected\"\n\n\
         [[actions]]\n\
         type = \"RunConnector\" | \"SendMessage\" | \"Notify\"\n\
         # For RunConnector:\n\
         connector = \"connector-name\"\n\
         action = \"action-name\"\n\
         [actions.params]\n\
         key = \"value\"\n\
         ```\n\n\
         IMPORTANT:\n\
         - Set enabled = false (the user will review and enable manually)\n\
         - Use only connectors and actions from the list below\n\
         - Output ONLY the TOML block, no explanation\n\n\
         Available connectors:\n",
    );

    for connector in connectors {
        prompt.push_str(&format!("\n## {}\n", connector.name));
        prompt.push_str(&format!("{}\n", connector.description));

        if !connector.triggers.is_empty() {
            prompt.push_str("Triggers:\n");
            for trigger in &connector.triggers {
                prompt.push_str(&format!("  - {} — {}\n", trigger.name, trigger.description));
            }
        }

        if !connector.actions.is_empty() {
            prompt.push_str("Actions:\n");
            for action in &connector.actions {
                prompt.push_str(&format!("  - {} — {}\n", action.name, action.description));
            }
        }
    }

    prompt
}

/// Build the user prompt from the intent string.
pub fn build_user_prompt(intent: &str) -> String {
    format!("Generate a TOML rule for: {intent}")
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::adapter::{ActionInfo, DisclosureLevel, TriggerInfo};

    #[test]
    fn test_system_prompt_includes_connectors() {
        let connectors = vec![ConnectorInfo {
            name: "connector-test".into(),
            description: "A test connector".into(),
            triggers: vec![TriggerInfo {
                name: "test_event".into(),
                description: "Test event fired".into(),
                schema: None,
            }],
            actions: vec![ActionInfo {
                name: "do_thing".into(),
                description: "Does a thing".into(),
                input_schema: None,
                output_schema: None,
            }],
            disclosure_level: DisclosureLevel::NamesAndDescriptions,
        }];

        let prompt = build_system_prompt(&connectors);
        assert!(prompt.contains("connector-test"));
        assert!(prompt.contains("test_event"));
        assert!(prompt.contains("do_thing"));
        assert!(prompt.contains("enabled = false"));
    }

    #[test]
    fn test_user_prompt() {
        let prompt = build_user_prompt("notify me when Kick stream goes live");
        assert!(prompt.contains("notify me when Kick stream goes live"));
    }
}
