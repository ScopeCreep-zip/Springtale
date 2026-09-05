//! Rule lifecycle operations — persist a rule (store + engine) before its
//! triggers ever go live, and tear its triggers down before the row that
//! backs them disappears.
//!
//! Finding 14: the old call order in `apps/springtaled/src/api/rules.rs`
//! scheduled/attached a rule's triggers BEFORE persisting it. If the store
//! insert then failed, the trigger was live with no row behind it — the
//! next boot would never know about it. These functions fix the order for
//! create, and give update/delete the same single call site so the daemon
//! and desktop can't drift (finding 13): store and engine first, triggers
//! second, on every surface.

use springtale_core::rule::types::{Rule, RuleId};

use crate::embedded::EmbeddedScheduler;
use crate::error::OperationError;
use crate::state::RuntimeState;
use crate::triggers::registry::TriggerRegistry;
use crate::triggers::{activate_rule, deactivate_rule};

use super::{create_rule, delete_rule, update_rule};

/// Create a rule and only then activate its triggers. If the store insert
/// fails, no trigger is ever scheduled or attached — there is nothing to
/// clean up on the next boot.
pub async fn create_and_activate(
    state: &RuntimeState,
    scheduler: &EmbeddedScheduler,
    registry: &TriggerRegistry,
    rule: Rule,
) -> Result<RuleId, OperationError> {
    let id = create_rule(state, rule.clone()).await?;
    activate_rule(&rule, scheduler, registry, &state.registry).await;
    Ok(id)
}

/// Replace a rule: deactivate the old rule's triggers (looked up from the
/// engine, as `api/rules.rs` did), persist the update, then activate the
/// new rule's triggers.
pub async fn update_and_reactivate(
    state: &RuntimeState,
    scheduler: &EmbeddedScheduler,
    registry: &TriggerRegistry,
    id: &RuleId,
    rule: Rule,
) -> Result<(), OperationError> {
    let old_rule = {
        let engine = state.engine.read().await;
        engine
            .list_rules()
            .iter()
            .find(|r| r.id == *id)
            .map(|r| (*r).clone())
    };
    if let Some(ref old) = old_rule {
        deactivate_rule(old, scheduler, registry, &state.registry).await;
    }

    update_rule(state, id, rule.clone()).await?;

    activate_rule(&rule, scheduler, registry, &state.registry).await;

    Ok(())
}

/// Deactivate a rule's triggers, then delete it (looked up from the
/// engine, as `api/rules.rs` did).
pub async fn delete_and_deactivate(
    state: &RuntimeState,
    scheduler: &EmbeddedScheduler,
    registry: &TriggerRegistry,
    id: &RuleId,
) -> Result<(), OperationError> {
    let old_rule = {
        let engine = state.engine.read().await;
        engine
            .list_rules()
            .iter()
            .find(|r| r.id == *id)
            .map(|r| (*r).clone())
    };
    if let Some(ref old) = old_rule {
        deactivate_rule(old, scheduler, registry, &state.registry).await;
    }

    delete_rule(state, id).await
}
