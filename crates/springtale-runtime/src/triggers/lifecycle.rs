//! Per-rule trigger activation — the single place a rule's triggers are
//! turned on or off on ANY surface.
//!
//! Two trigger families need per-rule registration at the app layer
//! (they're not driven by the store alone):
//!   - cron / filesystem-watch → [`EmbeddedScheduler`]
//!   - `ConnectorEvent` subscriptions → [`TriggerRegistry`]
//!
//! Both the daemon's HTTP rule handlers and the desktop's Tauri rule
//! commands call these two functions so a rule created/enabled/deleted
//! on either surface activates EXACTLY the same triggers. Webhook and
//! SystemEvent triggers need no per-rule registration — they're driven
//! by the webhook ingress and the heartbeat monitor respectively.

use std::sync::Arc;

use tokio::sync::RwLock;

use springtale_connector::registry::store::ConnectorRegistry;
use springtale_core::rule::types::Rule;

use crate::embedded::EmbeddedScheduler;
use crate::triggers::registry::TriggerRegistry;

/// Activate a rule's triggers: schedule its cron/filewatch trigger AND
/// attach its connector-event handler. Safe to call for any trigger
/// type — non-matching families are no-ops. A scheduling failure is
/// logged, not propagated, so one bad cron expression doesn't block the
/// rest of a rule's activation.
pub async fn activate_rule(
    rule: &Rule,
    scheduler: &EmbeddedScheduler,
    registry: &TriggerRegistry,
    connectors: &Arc<RwLock<ConnectorRegistry>>,
) {
    if let Err(e) = scheduler.schedule(rule).await {
        tracing::warn!(rule = %rule.name, error = %e, "failed to schedule rule trigger");
    }
    registry.attach_rule(rule, connectors).await;
}

/// Deactivate a rule's triggers: unschedule its cron/filewatch trigger
/// AND detach its connector-event handler. Idempotent — deactivating a
/// rule whose triggers were never registered is a no-op.
pub async fn deactivate_rule(
    rule: &Rule,
    scheduler: &EmbeddedScheduler,
    registry: &TriggerRegistry,
    connectors: &Arc<RwLock<ConnectorRegistry>>,
) {
    scheduler.unschedule(rule).await;
    registry.detach_rule(&rule.id, connectors).await;
}
