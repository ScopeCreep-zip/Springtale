//! Data operations — export and purge user data.

use serde::Serialize;
use springtale_core::rule::types::Rule;
use springtale_store::StorageBackend;
use springtale_store::schema::connectors::ConnectorRow;
use springtale_store::schema::events::{EventEntry, EventFilter};

use crate::error::OperationError;

/// Exported data snapshot.
#[derive(Debug, Serialize)]
pub struct DataExport {
    /// All automation rules.
    pub rules: Vec<Rule>,
    /// All registered connectors.
    pub connectors: Vec<ConnectorRow>,
    /// Recent events (up to 10,000).
    pub events: Vec<EventEntry>,
}

/// Export all user data as a serializable snapshot.
pub async fn export_data(store: &dyn StorageBackend) -> Result<DataExport, OperationError> {
    let rules = store.list_rules().await.map_err(OperationError::Store)?;
    let connectors = store
        .list_connectors()
        .await
        .map_err(OperationError::Store)?;
    let events = store
        .list_events(&EventFilter {
            limit: Some(10_000),
            ..Default::default()
        })
        .await
        .map_err(OperationError::Store)?;
    Ok(DataExport {
        rules,
        connectors,
        events,
    })
}

/// Purge all user data from the store without destroying the vault.
///
/// Deletes rules, events, sessions, memory, and connectors.
/// The vault file and its encryption remain intact.
pub async fn purge_data(store: &dyn StorageBackend) -> Result<(), OperationError> {
    // Delete all rules
    let rules = store.list_rules().await.map_err(OperationError::Store)?;
    for rule in &rules {
        store
            .delete_rule(&rule.id)
            .await
            .map_err(OperationError::Store)?;
    }
    // Delete all connectors
    let connectors = store
        .list_connectors()
        .await
        .map_err(OperationError::Store)?;
    for connector in &connectors {
        store
            .remove_connector(&connector.name)
            .await
            .map_err(OperationError::Store)?;
    }
    // Clear sessions and memory
    let sessions = store.list_sessions().await.map_err(OperationError::Store)?;
    for session in &sessions {
        store
            .delete_session(&session.user_id, &session.channel_id)
            .await
            .map_err(OperationError::Store)?;
        store
            .delete_memory(&session.user_id, &session.channel_id)
            .await
            .map_err(OperationError::Store)?;
    }
    Ok(())
}

/// Purge expired events and audit logs based on retention policy.
///
/// Called periodically (hourly) when `retention_days` is configured.
/// For IPV survivors: automatic data minimization reduces what an
/// adversary can recover from a seized device.
pub async fn purge_expired_data(
    store: &dyn StorageBackend,
    retention_days: u32,
) -> Result<u64, OperationError> {
    let cutoff = chrono::Utc::now() - chrono::TimeDelta::days(i64::from(retention_days));
    let events_deleted = store
        .delete_events_before(&cutoff)
        .await
        .map_err(OperationError::Store)?;
    let audit_deleted = store
        .delete_audit_before(&cutoff)
        .await
        .map_err(OperationError::Store)?;
    tracing::info!(
        events = events_deleted,
        audit = audit_deleted,
        retention_days,
        "expired data purged"
    );
    Ok(events_deleted + audit_deleted)
}
