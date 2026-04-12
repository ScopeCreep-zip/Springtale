use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use tokio::sync::{Mutex, RwLock, mpsc};

use springtale_connector::Subscription;
use springtale_connector::connector::trait_::EventHandler;
use springtale_connector::registry::store::ConnectorRegistry;
use springtale_core::rule::engine::{RuleEngine, TriggerEvent};
use springtale_core::rule::trigger::Trigger;
use springtale_core::rule::types::{Rule, RuleId, RuleStatus};

/// Active subscription with the connector name it was registered on.
pub(crate) struct ActiveSub {
    pub(crate) connector: String,
    pub(crate) subscription: Subscription,
}

/// Stores active event subscriptions per rule for lifecycle management.
///
/// Pattern: Home Assistant's `_async_detach_triggers` — stores detach
/// handles per automation. On rule disable/delete/update, iterate and
/// call `remove_event()` for each subscription.
///
/// n8n equivalent: `ActiveWorkflows.triggerResponses` map.
#[derive(Clone)]
pub struct TriggerRegistry {
    /// rule_id → list of active subscriptions to tear down on disable/delete
    active: Arc<Mutex<HashMap<RuleId, Vec<ActiveSub>>>>,
    /// Sender for trigger events — cloned into each handler closure
    trigger_tx: mpsc::Sender<TriggerEvent>,
}

impl TriggerRegistry {
    pub fn new(trigger_tx: mpsc::Sender<TriggerEvent>) -> Self {
        Self {
            active: Arc::new(Mutex::new(HashMap::new())),
            trigger_tx,
        }
    }

    /// Attach event handlers for a single rule's ConnectorEvent trigger.
    ///
    /// Called at boot for all enabled rules, and at runtime when a new
    /// ConnectorEvent rule is created or re-enabled.
    pub async fn attach_rule(
        &self,
        rule: &Rule,
        registry: &Arc<RwLock<ConnectorRegistry>>,
    ) {
        let (connector_name, event_name) = match &rule.trigger {
            Trigger::ConnectorEvent {
                connector, event, ..
            } => (connector.clone(), event.clone()),
            _ => return, // Not a ConnectorEvent trigger — nothing to wire
        };

        if rule.status != RuleStatus::Enabled {
            return;
        }

        let tx = self.trigger_tx.clone();
        let conn = connector_name.clone();
        let evt = event_name.clone();

        let handler: EventHandler = Box::new(move |payload| {
            let trigger = TriggerEvent {
                trigger_type: "ConnectorEvent".into(),
                connector: Some(conn.clone()),
                event: Some(evt.clone()),
                payload,
            };
            let tx = tx.clone();
            tokio::spawn(async move {
                if let Err(e) = tx.send(trigger).await {
                    tracing::warn!(error = %e, "failed to send connector trigger event");
                }
            });
        });

        let reg = registry.read().await;
        if let Some(entry) = reg.get(&connector_name) {
            match entry.host.on_event(&event_name, handler).await {
                Ok(sub) => {
                    tracing::info!(
                        rule = %rule.name,
                        connector = %connector_name,
                        event = %event_name,
                        "attached connector event handler"
                    );
                    let mut active = self.active.lock().await;
                    active.entry(rule.id).or_default().push(ActiveSub {
                        connector: connector_name.clone(),
                        subscription: sub,
                    });
                }
                Err(e) => {
                    tracing::warn!(
                        rule = %rule.name,
                        connector = %connector_name,
                        event = %event_name,
                        error = %e,
                        "failed to attach connector event handler"
                    );
                }
            }
        } else {
            tracing::warn!(
                rule = %rule.name,
                connector = %connector_name,
                "connector not found — cannot attach event handler"
            );
        }
    }

    /// Detach all event handlers for a rule.
    ///
    /// Called when a rule is disabled, deleted, or about to be updated
    /// (update = detach old + attach new, per HA/n8n pattern).
    pub async fn detach_rule(
        &self,
        rule_id: &RuleId,
        registry: &Arc<RwLock<ConnectorRegistry>>,
    ) {
        let subs = {
            let mut active = self.active.lock().await;
            active.remove(rule_id).unwrap_or_default()
        };

        if subs.is_empty() {
            return;
        }

        let reg = registry.read().await;
        for active_sub in &subs {
            if let Some(entry) = reg.get(&active_sub.connector)
                && let Err(e) = entry.host.remove_event(&active_sub.subscription).await
            {
                tracing::warn!(
                    connector = %active_sub.connector,
                    id = ?active_sub.subscription.id,
                    error = %e,
                    "failed to remove event handler"
                );
            }
        }

        tracing::info!(
            rule_id = %rule_id.0,
            count = subs.len(),
            "detached connector event handlers"
        );
    }
}

/// Wire connector event handlers for ALL enabled ConnectorEvent rules at boot.
///
/// Returns the TriggerRegistry for use in runtime rule management
/// (create/update/toggle/delete API handlers call attach_rule/detach_rule).
pub(super) async fn wire_connector_events(
    registry: &Arc<RwLock<ConnectorRegistry>>,
    engine: &Arc<RwLock<RuleEngine>>,
    trigger_tx: mpsc::Sender<TriggerEvent>,
) -> TriggerRegistry {
    let trigger_registry = TriggerRegistry::new(trigger_tx);

    let rules: Vec<Rule> = {
        let engine_guard = engine.read().await;
        engine_guard.list_rules().into_iter().cloned().collect()
    };

    // Collect unique ConnectorEvent rules
    let connector_rules: Vec<_> = rules
        .iter()
        .filter(|r| {
            matches!(r.trigger, Trigger::ConnectorEvent { .. })
                && r.status == RuleStatus::Enabled
        })
        .collect::<Vec<_>>();

    if connector_rules.is_empty() {
        tracing::debug!("no ConnectorEvent rules — skipping event handler wiring");
        return trigger_registry;
    }

    // Deduplicate by (connector, event) to avoid registering multiple handlers
    // for the same event. Multiple rules can share one handler — the rule engine
    // handles matching at evaluation time.
    let mut seen: HashSet<(String, String)> = HashSet::new();
    for rule in &connector_rules {
        if let Trigger::ConnectorEvent {
            connector, event, ..
        } = &rule.trigger
            && seen.insert((connector.clone(), event.clone()))
        {
            trigger_registry.attach_rule(rule, registry).await;
        }
    }

    let active = trigger_registry.active.lock().await;
    let wired: usize = active.values().map(|v| v.len()).sum();
    tracing::info!(
        wired = wired,
        rules = connector_rules.len(),
        "connector event handlers wired at boot"
    );
    drop(active);

    trigger_registry
}
