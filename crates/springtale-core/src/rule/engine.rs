use std::collections::HashMap;
use std::sync::Arc;

use regex::{Regex, RegexBuilder};

use super::action::Action;
use super::condition::Condition;
use super::evaluate::evaluate_condition;
use super::trigger::Trigger;
use super::types::{Rule, RuleId, RuleStatus};
use crate::error::CoreError;

/// Maximum compiled regex size (same as evaluate.rs).
const REGEX_SIZE_LIMIT: usize = 1_000_000;

/// Cache of compiled regexes, keyed by pattern string.
/// Shared between rule engine and condition evaluation.
pub type RegexCache = HashMap<String, Regex>;

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
///
/// Actions and payload are `Arc`-wrapped to avoid deep-cloning per rule match.
/// The engine's `evaluate()` creates one `Arc<Value>` per event and one
/// `Arc<Vec<Action>>` per rule, so matches are cheap ref-count bumps.
#[derive(Debug)]
pub struct RuleMatch {
    /// The rule that matched.
    pub rule_id: RuleId,
    /// The rule name (for logging).
    pub rule_name: String,
    /// The actions to dispatch (Arc-wrapped to avoid deep cloning).
    pub actions: Arc<Vec<Action>>,
    /// The trigger payload (Arc-wrapped to avoid deep cloning).
    pub payload: Arc<serde_json::Value>,
}

/// The rule engine: loads rules, matches triggers, evaluates conditions.
///
/// No AI dependency. No network. Pure evaluation logic.
/// Action dispatch is NOT handled here — that's the application layer's job.
pub struct RuleEngine {
    rules: HashMap<RuleId, Rule>,
    /// Compiled regex cache keyed by pattern string. Populated at `add_rule()`
    /// time, consulted during condition evaluation. Avoids recompiling regexes
    /// on every trigger event.
    regex_cache: RegexCache,
}

impl RuleEngine {
    pub fn new() -> Self {
        Self {
            rules: HashMap::new(),
            regex_cache: HashMap::new(),
        }
    }

    /// Add a rule to the engine.
    ///
    /// Pre-compiles all Regex condition patterns and caches them. Returns
    /// an error if any regex pattern is invalid (fail fast at rule load time
    /// rather than silently returning false during evaluation).
    pub fn add_rule(&mut self, rule: Rule) -> Result<(), CoreError> {
        // Pre-compile and cache all regex patterns in this rule's conditions
        for condition in &rule.conditions {
            precompile_condition_regexes(condition, &mut self.regex_cache)?;
        }
        self.rules.insert(rule.id, rule);
        Ok(())
    }

    /// Remove a rule from the engine.
    ///
    /// Note: does not remove regex patterns from cache. Patterns may be shared
    /// across rules, and the cache is cheap (just compiled regexes).
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
    ///
    /// This entry point applies no ownership filter — all matching
    /// rules fire regardless of [`super::RuleOwner`]. Pre-Phase-0
    /// behavior; preserved for callers that don't yet thread firing
    /// context (e.g. NL-rule preview, ad-hoc evaluator tests).
    /// Production callers should use
    /// [`Self::evaluate_with_filter`] so cross-formation rules don't
    /// fire from the wrong context.
    pub fn evaluate(&self, event: &TriggerEvent) -> Vec<RuleMatch> {
        self.evaluate_with_filter(event, None, None)
    }

    /// Cooperation-aware variant of [`Self::evaluate`]. Filters out
    /// rules whose [`super::RuleOwner`] doesn't match the firing
    /// context's agent / formation ids.
    ///
    /// Lookup semantics:
    ///   - `RuleOwner::Global` always matches (global rules fire
    ///     regardless of context).
    ///   - `RuleOwner::Agent { id }` matches when `agent_id == Some(id)`.
    ///   - `RuleOwner::Formation { id }` matches when
    ///     `formation_id == Some(id)`.
    ///
    /// Pass `None` for both args to fire only Global rules — this is
    /// the "daemon queue / system cron" semantics. Pass `agent_id` to
    /// also fire Agent rules for that agent. Pass both to fire all
    /// three.
    pub fn evaluate_with_filter(
        &self,
        event: &TriggerEvent,
        agent_id: Option<uuid::Uuid>,
        formation_id: Option<uuid::Uuid>,
    ) -> Vec<RuleMatch> {
        let payload = Arc::new(event.payload.clone());

        self.rules
            .values()
            .filter(|rule| rule.status == RuleStatus::Enabled)
            .filter(|rule| rule.owner.matches(agent_id, formation_id))
            .filter(|rule| trigger_matches(&rule.trigger, event))
            .filter(|rule| {
                rule.conditions
                    .iter()
                    .all(|c| evaluate_condition(c, &event.payload, &self.regex_cache))
            })
            .map(|rule| RuleMatch {
                rule_id: rule.id,
                rule_name: rule.name.clone(),
                actions: Arc::new(rule.actions.clone()),
                payload: Arc::clone(&payload),
            })
            .collect()
    }

    /// Get a reference to the regex cache (for testing).
    #[cfg(test)]
    pub fn regex_cache(&self) -> &RegexCache {
        &self.regex_cache
    }
}

/// Recursively walk a condition tree and compile all Regex patterns into the cache.
fn precompile_condition_regexes(
    condition: &Condition,
    cache: &mut RegexCache,
) -> Result<(), CoreError> {
    match condition {
        Condition::Regex { pattern, .. } => {
            if !cache.contains_key(pattern) {
                let compiled = RegexBuilder::new(pattern)
                    .size_limit(REGEX_SIZE_LIMIT)
                    .build()
                    .map_err(|e| {
                        CoreError::RuleParse(format!("invalid regex pattern '{pattern}': {e}"))
                    })?;
                cache.insert(pattern.clone(), compiled);
            }
            Ok(())
        }
        Condition::And { conditions } | Condition::Or { conditions } => {
            for c in conditions {
                precompile_condition_regexes(c, cache)?;
            }
            Ok(())
        }
        Condition::Not { condition } => precompile_condition_regexes(condition, cache),
        _ => Ok(()),
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
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
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
            owner: super::super::types::RuleOwner::Global,
            actions: vec![Action::SendMessage {
                text: "matched!".into(),
            }],
        }
    }

    #[test]
    fn test_engine_matches_connector_event() {
        let mut engine = RuleEngine::new();
        engine
            .add_rule(make_rule("test", "connector-kick", "stream_live"))
            .unwrap();

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
        engine
            .add_rule(make_rule("test", "connector-kick", "stream_live"))
            .unwrap();

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
        engine.add_rule(rule).unwrap();

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
        engine.add_rule(rule).unwrap();

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

    #[test]
    fn test_regex_conditions_cached_at_add_time() {
        let mut engine = RuleEngine::new();
        let mut rule = make_rule("regex-rule", "connector-kick", "stream_live");
        rule.conditions = vec![Condition::Regex {
            field: "title".into(),
            pattern: r"\.(pdf|docx)$".into(),
        }];
        engine.add_rule(rule).unwrap();

        // Cache should contain the compiled regex
        assert!(engine.regex_cache().contains_key(r"\.(pdf|docx)$"));
    }

    #[test]
    fn test_invalid_regex_rejected_at_add_time() {
        let mut engine = RuleEngine::new();
        let mut rule = make_rule("bad-regex", "connector-kick", "stream_live");
        rule.conditions = vec![Condition::Regex {
            field: "title".into(),
            pattern: r"[invalid".into(),
        }];

        let result = engine.add_rule(rule);
        assert!(
            result.is_err(),
            "should reject invalid regex at add_rule time"
        );
        let err = result.err().map(|e| e.to_string()).unwrap_or_default();
        assert!(err.contains("invalid regex"), "error: {err}");
    }

    #[test]
    fn test_regex_evaluation_uses_cache() {
        let mut engine = RuleEngine::new();
        let mut rule = make_rule("regex-rule", "connector-kick", "stream_live");
        rule.conditions = vec![Condition::Regex {
            field: "filename".into(),
            pattern: r"\.(pdf|docx)$".into(),
        }];
        engine.add_rule(rule).unwrap();

        let event = TriggerEvent {
            trigger_type: "ConnectorEvent".into(),
            connector: Some("connector-kick".into()),
            event: Some("stream_live".into()),
            payload: json!({"filename": "report.pdf"}),
        };
        assert_eq!(engine.evaluate(&event).len(), 1);

        let event_no_match = TriggerEvent {
            trigger_type: "ConnectorEvent".into(),
            connector: Some("connector-kick".into()),
            event: Some("stream_live".into()),
            payload: json!({"filename": "image.png"}),
        };
        assert!(engine.evaluate(&event_no_match).is_empty());
    }
}
