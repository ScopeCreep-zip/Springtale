use std::collections::HashMap;

use super::evaluate::evaluate_condition;
use super::trigger::Trigger;
use super::types::{Rule, RuleId, RuleStatus};

/// Event payload from a trigger source.
#[derive(Debug, Clone)]
pub struct TriggerEvent {
    /// What kind of trigger fired.
    pub trigger_type: String,
    /// Connector name (for ConnectorEvent triggers).
    pub connector: Option<String>,
    /// Event name (for ConnectorEvent triggers).
    pub event: Option<String>,
    /// The payload data.
    pub payload: serde_json::Value,
}

/// Result of evaluating a rule against a trigger event.
#[derive(Debug)]
pub struct RuleMatch {
    /// The rule that matched.
    pub rule_id: RuleId,
    /// The rule name (for logging).
    pub rule_name: String,
    /// The actions to dispatch.
    pub actions: Vec<super::action::Action>,
    /// The trigger payload (for template resolution).
    pub payload: serde_json::Value,
}

/// The rule engine: loads rules, matches triggers, evaluates conditions.
///
/// No AI dependency. No network. Pure evaluation logic.
/// Action dispatch is NOT handled here — that's the application layer's job.
pub struct RuleEngine {
    rules: HashMap<RuleId, Rule>,
}

impl RuleEngine {
    pub fn new() -> Self {
        Self {
            rules: HashMap::new(),
        }
    }

    /// Add a rule to the engine.
    pub fn add_rule(&mut self, rule: Rule) {
        self.rules.insert(rule.id, rule);
    }

    /// Remove a rule from the engine.
    pub fn remove_rule(&mut self, id: &RuleId) -> Option<Rule> {
        self.rules.remove(id)
    }

    /// Toggle a rule's status.
    pub fn set_status(&mut self, id: &RuleId, status: RuleStatus) -> bool {
        if let Some(rule) = self.rules.get_mut(id) {
            rule.status = status;
            true
        } else {
            false
        }
    }

    /// List all rules.
    pub fn list_rules(&self) -> Vec<&Rule> {
        self.rules.values().collect()
    }

    /// Evaluate a trigger event against all enabled rules.
    ///
    /// Returns a list of rules whose trigger matched AND whose conditions
    /// all passed. The caller is responsible for dispatching the actions.
    pub fn evaluate(&self, event: &TriggerEvent) -> Vec<RuleMatch> {
        self.rules
            .values()
            .filter(|rule| rule.status == RuleStatus::Enabled)
            .filter(|rule| trigger_matches(&rule.trigger, event))
            .filter(|rule| {
                rule.conditions
                    .iter()
                    .all(|c| evaluate_condition(c, &event.payload))
            })
            .map(|rule| RuleMatch {
                rule_id: rule.id,
                rule_name: rule.name.clone(),
                actions: rule.actions.clone(),
                payload: event.payload.clone(),
            })
            .collect()
    }
}

impl Default for RuleEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Check if a trigger definition matches an incoming event.
fn trigger_matches(trigger: &Trigger, event: &TriggerEvent) -> bool {
    match trigger {
        Trigger::Cron { .. } => event.trigger_type == "Cron",
        Trigger::FileWatch { .. } => event.trigger_type == "FileWatch",
        Trigger::Webhook { path } => {
            event.trigger_type == "Webhook" && event.event.as_deref() == Some(path.as_str())
        }
        Trigger::ConnectorEvent {
            connector,
            event: ev,
        } => {
            event.trigger_type == "ConnectorEvent"
                && event.connector.as_deref() == Some(connector.as_str())
                && event.event.as_deref() == Some(ev.as_str())
        }
        Trigger::SystemEvent { event: ev } => {
            event.trigger_type == "SystemEvent" && event.event.as_deref() == Some(ev.as_str())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::action::Action;
    use crate::rule::condition::Condition;
    use serde_json::json;

    fn make_rule(name: &str, connector: &str, event: &str) -> Rule {
        Rule {
            id: super::super::types::RuleId::new(),
            name: name.into(),
            description: String::new(),
            status: RuleStatus::Enabled,
            version: super::super::types::RuleVersion(1),
            trigger: Trigger::ConnectorEvent {
                connector: connector.into(),
                event: event.into(),
            },
            conditions: vec![],
            actions: vec![Action::SendMessage {
                text: "matched!".into(),
            }],
        }
    }

    #[test]
    fn test_engine_matches_connector_event() {
        let mut engine = RuleEngine::new();
        engine.add_rule(make_rule("test", "connector-kick", "stream_live"));

        let event = TriggerEvent {
            trigger_type: "ConnectorEvent".into(),
            connector: Some("connector-kick".into()),
            event: Some("stream_live".into()),
            payload: json!({}),
        };

        let matches = engine.evaluate(&event);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].rule_name, "test");
    }

    #[test]
    fn test_engine_no_match_wrong_event() {
        let mut engine = RuleEngine::new();
        engine.add_rule(make_rule("test", "connector-kick", "stream_live"));

        let event = TriggerEvent {
            trigger_type: "ConnectorEvent".into(),
            connector: Some("connector-kick".into()),
            event: Some("chat_message".into()),
            payload: json!({}),
        };

        let matches = engine.evaluate(&event);
        assert!(matches.is_empty());
    }

    #[test]
    fn test_engine_skips_disabled_rules() {
        let mut engine = RuleEngine::new();
        let mut rule = make_rule("disabled", "connector-kick", "stream_live");
        rule.status = RuleStatus::Disabled;
        let id = rule.id;
        engine.add_rule(rule);

        let event = TriggerEvent {
            trigger_type: "ConnectorEvent".into(),
            connector: Some("connector-kick".into()),
            event: Some("stream_live".into()),
            payload: json!({}),
        };

        assert!(engine.evaluate(&event).is_empty());

        // Re-enable and verify it matches
        engine.set_status(&id, RuleStatus::Enabled);
        assert_eq!(engine.evaluate(&event).len(), 1);
    }

    #[test]
    fn test_engine_conditions_filter() {
        let mut engine = RuleEngine::new();
        let mut rule = make_rule("filtered", "connector-kick", "stream_live");
        rule.conditions = vec![Condition::FieldEquals {
            field: "category".into(),
            value: json!("gaming"),
        }];
        engine.add_rule(rule);

        // Matching payload
        let event = TriggerEvent {
            trigger_type: "ConnectorEvent".into(),
            connector: Some("connector-kick".into()),
            event: Some("stream_live".into()),
            payload: json!({"category": "gaming"}),
        };
        assert_eq!(engine.evaluate(&event).len(), 1);

        // Non-matching payload
        let event = TriggerEvent {
            trigger_type: "ConnectorEvent".into(),
            connector: Some("connector-kick".into()),
            event: Some("stream_live".into()),
            payload: json!({"category": "music"}),
        };
        assert!(engine.evaluate(&event).is_empty());
    }
}
