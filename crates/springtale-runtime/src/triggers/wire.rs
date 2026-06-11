//! Boot-time wiring of `ConnectorEvent` rules.

use std::collections::HashSet;
use std::sync::Arc;

use tokio::sync::{RwLock, mpsc};

use springtale_connector::registry::store::ConnectorRegistry;
use springtale_core::rule::engine::{RuleEngine, TriggerEvent};
use springtale_core::rule::trigger::Trigger;
use springtale_core::rule::types::{Rule, RuleStatus};

use super::registry::TriggerRegistry;

/// Wire connector event handlers for ALL enabled ConnectorEvent rules at boot.
///
/// Returns the [`TriggerRegistry`] for use in runtime rule management
/// (create/update/toggle/delete handlers call `attach_rule`/`detach_rule`).
/// Both `springtaled` and the desktop shell call this at boot so connector
/// emissions reach the rule engine on every surface.
pub async fn wire_connector_events(
    registry: &Arc<RwLock<ConnectorRegistry>>,
    engine: &Arc<RwLock<RuleEngine>>,
    trigger_tx: mpsc::Sender<TriggerEvent>,
    store: Arc<dyn springtale_store::StorageBackend>,
) -> TriggerRegistry {
    let trigger_registry = TriggerRegistry::new(trigger_tx, store);

    let rules: Vec<Rule> = {
        let engine_guard = engine.read().await;
        engine_guard.list_rules().into_iter().cloned().collect()
    };

    // Collect unique ConnectorEvent rules
    let connector_rules: Vec<_> = rules
        .iter()
        .filter(|r| {
            matches!(r.trigger, Trigger::ConnectorEvent { .. }) && r.status == RuleStatus::Enabled
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

    // NB: resolve the count BEFORE the tracing macro — awaiting inside a
    // `tracing::` argument list holds non-Send `dyn Value` refs across the
    // await, which makes every caller's future !Send (Tauri commands
    // require Send futures).
    let wired = trigger_registry.active_rule_count().await;
    tracing::info!(
        wired = wired,
        rules = connector_rules.len(),
        "connector event handlers wired at boot"
    );

    trigger_registry
}
