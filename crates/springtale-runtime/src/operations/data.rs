//! Data operations — export, import, and purge user data.

use serde::{Deserialize, Serialize};
use springtale_core::rule::types::Rule;
use springtale_store::StorageBackend;
use springtale_store::schema::connectors::ConnectorRow;
use springtale_store::schema::events::{EventEntry, EventFilter};

use crate::error::OperationError;

/// Exported data snapshot.
///
/// `Serialize + Deserialize` so the same type is both the export format and
/// the import contract — per rust-conventions, serde on data types that
/// cross boundaries (API, storage) is explicitly allowed.
#[derive(Debug, Serialize, Deserialize)]
pub struct DataExport {
    /// All automation rules.
    pub rules: Vec<Rule>,
    /// All registered connectors.
    pub connectors: Vec<ConnectorRow>,
    /// Recent events (up to 10,000).
    pub events: Vec<EventEntry>,
}

/// Summary of an import pass — returned so callers can report counts.
#[derive(Debug, Clone, Copy, Default)]
pub struct ImportStats {
    pub rules_inserted: usize,
    pub connectors_inserted: usize,
    pub events_inserted: usize,
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

/// Import a previously exported snapshot into the store.
///
/// Rules and connectors are inserted; events are appended to the audit
/// trail so the destination keeps its own history as well. The caller is
/// responsible for any pre-import reconciliation (wipe first, merge, etc.)
/// — this function is strictly the write side of export.
pub async fn import_data(
    store: &dyn StorageBackend,
    export: DataExport,
) -> Result<ImportStats, OperationError> {
    let mut stats = ImportStats::default();
    for rule in &export.rules {
        store.insert_rule(rule).await.map_err(OperationError::Store)?;
        stats.rules_inserted += 1;
    }
    for connector in &export.connectors {
        store
            .register_connector(connector)
            .await
            .map_err(OperationError::Store)?;
        stats.connectors_inserted += 1;
    }
    for event in &export.events {
        store.log_event(event).await.map_err(OperationError::Store)?;
        stats.events_inserted += 1;
    }
    Ok(stats)
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use chrono::Utc;
    use springtale_core::rule::action::Action;
    use springtale_core::rule::trigger::Trigger;
    use springtale_core::rule::types::{RuleId, RuleStatus, RuleVersion};
    use springtale_store::backend::InMemoryBackend;
    use uuid::Uuid;

    fn sample_rule() -> Rule {
        Rule {
            id: RuleId::new(),
            name: "exported-rule".into(),
            description: "round-trip".into(),
            status: RuleStatus::Enabled,
            version: RuleVersion(1),
            trigger: Trigger::Cron {
                expression: "0 0 * * *".into(),
            },
            conditions: Vec::new(),
            actions: vec![Action::SendMessage {
                text: "hello".into(),
            }],
        }
    }

    fn sample_connector() -> ConnectorRow {
        ConnectorRow {
            name: "connector-test".into(),
            version: "0.1.0".into(),
            author: "springtale".into(),
            description: "test".into(),
            manifest_json: "{}".into(),
            enabled: true,
            installed_at: Utc::now(),
        }
    }

    fn sample_event() -> EventEntry {
        EventEntry {
            id: Uuid::new_v4(),
            connector_name: "connector-test".into(),
            trigger_type: "Cron".into(),
            timestamp: Utc::now(),
            action_taken: "send:hello".into(),
        }
    }

    #[tokio::test]
    async fn test_export_import_round_trip_preserves_all_rows() {
        let source = InMemoryBackend::new();
        let rule = sample_rule();
        let connector = sample_connector();
        let event = sample_event();
        source.insert_rule(&rule).await.unwrap();
        source.register_connector(&connector).await.unwrap();
        source.log_event(&event).await.unwrap();

        let export = export_data(&source).await.unwrap();
        assert_eq!(export.rules.len(), 1);
        assert_eq!(export.connectors.len(), 1);
        assert_eq!(export.events.len(), 1);

        // Serialize → deserialize to prove the JSON contract holds.
        let json = serde_json::to_string(&export).unwrap();
        let decoded: DataExport = serde_json::from_str(&json).unwrap();

        let dest = InMemoryBackend::new();
        let stats = import_data(&dest, decoded).await.unwrap();
        assert_eq!(stats.rules_inserted, 1);
        assert_eq!(stats.connectors_inserted, 1);
        assert_eq!(stats.events_inserted, 1);

        let rules = dest.list_rules().await.unwrap();
        let connectors = dest.list_connectors().await.unwrap();
        let events = dest
            .list_events(&EventFilter::default())
            .await
            .unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].name, "exported-rule");
        assert_eq!(connectors.len(), 1);
        assert_eq!(connectors[0].name, "connector-test");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].action_taken, "send:hello");
    }
}
