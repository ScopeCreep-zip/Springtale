use crate::rule::engine::{RuleEngine, RuleMatch, TriggerEvent};

/// Dispatch a trigger event through the rule engine.
///
/// Returns all matching rules with their actions. The caller (springtaled
/// or springtale-bot) is responsible for executing the actions through
/// the connector framework and pipeline engine.
///
/// No cooperation filter — fires all matching rules regardless of
/// [`crate::rule::types::RuleOwner`]. Equivalent to passing
/// `(None, None)` to [`dispatch_event_with_owner`]. Use the filtered
/// variant when the caller has agent / formation context (e.g.
/// formation-tick path).
pub fn dispatch_event(engine: &RuleEngine, event: &TriggerEvent) -> Vec<RuleMatch> {
    dispatch_event_with_owner(engine, event, None, None)
}

/// Cooperation-aware variant. Filters rules whose
/// [`crate::rule::types::RuleOwner`] doesn't match the firing
/// context's agent / formation ids.
///
/// Pass:
///   - `(None, None)` — fire only Global rules. Daemon-queue /
///     system-cron path.
///   - `(Some(agent), None)` — fire Global + matching Agent rules.
///     Solo-bot path with agent identity.
///   - `(_, Some(formation))` — fire Global + matching Formation
///     rules. Formation-tick path.
///   - `(Some(agent), Some(formation))` — fire all three flavors
///     that apply.
pub fn dispatch_event_with_owner(
    engine: &RuleEngine,
    event: &TriggerEvent,
    agent_id: Option<uuid::Uuid>,
    formation_id: Option<uuid::Uuid>,
) -> Vec<RuleMatch> {
    let matches = engine.evaluate_with_filter(event, agent_id, formation_id);

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
                owner: crate::rule::types::RuleOwner::Global,
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
