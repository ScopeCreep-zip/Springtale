use std::sync::Arc;

use springtale_store::StorageBackend;

use crate::error::SentinelError;
use crate::verdict::Verdict;

/// Append-only audit trail backed by SQLite.
///
/// Every action evaluated by the sentinel is logged here, along with
/// the verdict and result. The trail is never modified — only appended.
pub struct AuditTrail {
    store: Arc<dyn StorageBackend>,
}

impl AuditTrail {
    pub fn new(store: Arc<dyn StorageBackend>) -> Self {
        Self { store }
    }

    /// Log an action evaluation to the audit trail.
    pub async fn log(
        &self,
        connector_name: &str,
        action_type: &str,
        action_summary: &str,
        verdict: &Verdict,
        result: &str,
    ) -> Result<(), SentinelError> {
        let (verdict_str, reason) = match verdict {
            Verdict::Go => ("go".to_owned(), String::new()),
            Verdict::Throttle(d) => ("throttle".to_owned(), format!("{}ms", d.as_millis())),
            Verdict::Pause(r) => ("pause".to_owned(), r.clone()),
            Verdict::Quarantine(r) => ("quarantine".to_owned(), r.clone()),
        };

        let entry = springtale_store::AuditEntry {
            id: uuid::Uuid::new_v4(),
            timestamp: chrono::Utc::now(),
            connector_name: connector_name.to_owned(),
            action_type: action_type.to_owned(),
            action_summary: action_summary.to_owned(),
            verdict: verdict_str,
            verdict_reason: reason,
            result: result.to_owned(),
        };

        self.store.insert_audit_entry(&entry).await?;
        Ok(())
    }
}
