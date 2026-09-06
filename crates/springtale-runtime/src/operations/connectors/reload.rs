//! G4 — connector hot-reload mid-mission.
//!
//! Replaces a connector's host atomically without dropping in-flight
//! calls. Reuses `setup_connector`'s factory-lookup path so the
//! configuration source (stored in `config_store` under `connector:{name}`)
//! drives the rebuilt connector — call sites pass only `name`.
//!
//! Pattern (per `COOPERATION_IMPLEMENTATION_PLAN.md §12.7`):
//!
//! 1. Read the persisted config so the rebuilt connector matches the
//!    last-applied user choices.
//! 2. Look up the factory by name + spin up a fresh `Connector` via
//!    `factory.create(config)`. Same path `setup_connector` uses, so
//!    manifest validation + capability declarations match exactly.
//! 3. Snapshot the current `enabled` state so the swap doesn't
//!    accidentally re-enable a deliberately-disabled connector.
//! 4. Under the registry write lock, `remove` the old entry and then
//!    `install_native` the rebuilt connector. A name is registered
//!    once (`ConnectorError::AlreadyRegistered`), so the remove must
//!    come first; both steps happen under the same write guard, so no
//!    reader observes the gap. `install_native` re-registers
//!    capabilities (`CapabilityChecker::register` overwrites the prior
//!    grant entry, so a reload is idempotent on the capability map).
//! 5. Restore the previously-captured `enabled` flag.
//!
//! Existing in-flight `execute()` calls that obtained the old host via
//! `registry.get_for_execute()` are holding an `Arc<dyn ConnectorHost>`
//! clone — they finish on the old instance and the `Arc` frees once
//! that last guard drops. Subsequent calls land on the new host.
//!
//! Idempotent: reloading a present connector always succeeds; reloading
//! a missing one fails fast before any rebuild work, so the registry's
//! invariants ("rebuild or roll back") hold either way.

use springtale_connector::factory::FactoryEntry;

use crate::error::OperationError;
use crate::state::RuntimeState;

pub async fn reload_connector(state: &RuntimeState, name: &str) -> Result<(), OperationError> {
    // 1. Verify the connector is actually installed + snapshot its
    //    current enabled state. Fail before we burn cycles on a
    //    factory rebuild for a missing target.
    let was_enabled = {
        let registry = state.registry.read().await;
        let entry = registry.get(name).ok_or_else(|| {
            OperationError::Validation(format!(
                "connector '{name}' is not installed; nothing to reload"
            ))
        })?;
        entry.enabled
    };

    // 2. Read the persisted config so the rebuilt connector matches the
    //    last-applied user state. Missing config is fine — connectors
    //    that don't require config use `serde_json::Value::Null`.
    let config = super::get_connector_config(&*state.store, name)
        .await
        .unwrap_or(serde_json::Value::Null);

    // 3. Find the factory by name / config_key and rebuild a fresh
    //    `Connector` instance. Mirrors `setup_connector`'s lookup so the
    //    same factory always rebuilds the same connector.
    let factory = inventory::iter::<FactoryEntry>
        .into_iter()
        .find(|e| e.factory.name() == name || e.factory.config_key() == name)
        .map(|e| e.factory)
        .ok_or_else(|| {
            OperationError::Validation(format!(
                "no factory found for connector '{name}'; cannot rebuild"
            ))
        })?;

    let fresh = factory
        .create(config)
        .await
        .map_err(|e| OperationError::Connector(format!("failed to rebuild {name}: {e}")))?;

    // 4. Swap under the registry write lock: remove the old entry, then
    //    install the rebuilt one. The registry refuses to register a
    //    name twice, so the remove must come first; the prior
    //    `Arc<dyn ConnectorHost>` drops once the last in-flight
    //    `execute()` guard finishes its work.
    // 5. Restore the previous `enabled` flag (install_native defaults
    //    to `enabled: true`, which would silently re-enable a
    //    deliberately-disabled connector if we didn't preserve state).
    {
        let mut registry = state.registry.write().await;
        registry
            .remove(name)
            .map_err(|e| OperationError::Connector(format!("failed to remove {name}: {e}")))?;
        registry
            .install_native(fresh)
            .map_err(|e| OperationError::Connector(format!("failed to reinstall {name}: {e}")))?;
        if !was_enabled {
            // Best-effort restore: the entry exists because we just
            // wrote it. The only failure path is `NotFound`, which
            // can't happen here.
            let _ = registry.disable(name);
        }
    }

    tracing::info!(connector = name, was_enabled, "connector hot-reloaded");
    // The rebuilt connector owns a fresh ChatSource — stop the old
    // loop and start the new one.
    super::chat::unwire_chat(state, name);
    super::chat::wire_chat(state, name).await?;

    Ok(())
}
