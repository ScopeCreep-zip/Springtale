use crate::rule::engine::{RuleEngine, RuleMatch, TriggerEvent};

/// Dispatch a trigger event through the rule engine.
///
/// Returns all matching rules with their actions. The caller (springtaled
/// or springtale-bot) is responsible for executing the actions through
/// the connector framework and pipeline engine.
pub fn dispatch_event(engine: &RuleEngine, event: &TriggerEvent) -> Vec<RuleMatch> {
    let matches = engine.evaluate(event);

    for m in &matches {
        tracing::info!(
            rule_id = %m.rule_id,
            rule_name = %m.rule_name,
            actions = m.actions.len(),
            "rule matched trigger event"
        );
    }

    matches
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::rule::action::Action;
    use crate::rule::trigger::Trigger;
    use crate::rule::types::{Rule, RuleId, RuleStatus, RuleVersion};
    use serde_json::json;

    #[test]
    fn test_dispatch_returns_matches() {
        let mut engine = RuleEngine::new();
        engine
            .add_rule(Rule {
                id: RuleId::new(),
                name: "test".into(),
                description: String::new(),
                status: RuleStatus::Enabled,
                version: RuleVersion(1),
                trigger: Trigger::ConnectorEvent {
                    connector: "connector-kick".into(),
                    event: "stream_live".into(),
                },
                conditions: vec![],
                actions: vec![Action::SendMessage {
                    text: "live!".into(),
                }],
            })
            .unwrap();

        let event = TriggerEvent {
            trigger_type: "ConnectorEvent".into(),
            connector: Some("connector-kick".into()),
            event: Some("stream_live".into()),
            payload: json!({}),
        };

        let matches = dispatch_event(&engine, &event);
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn test_dispatch_empty_on_no_match() {
        let engine = RuleEngine::new();
        let event = TriggerEvent {
            trigger_type: "ConnectorEvent".into(),
            connector: Some("connector-kick".into()),
            event: Some("stream_live".into()),
            payload: json!({}),
        };
        assert!(dispatch_event(&engine, &event).is_empty());
    }
}
