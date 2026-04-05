use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tokio::sync::RwLock;

use crate::error::StoreError;
use crate::schema::audit::{AuditEntry, AuditFilter};
use crate::schema::bot::{MemoryRow, SessionRow, UserPrefsRow};
use crate::schema::connectors::ConnectorRow;
use crate::schema::events::{EventEntry, EventFilter};
use crate::schema::jobs::{JobId, JobRow};
use crate::schema::formations::{FormationMemberRow, FormationRow};
use crate::schema::safety::SafetyConfigRow;
use springtale_core::rule::types::{Rule, RuleId};

use super::trait_::StorageBackend;

/// In-memory storage backend for ephemeral mode.
///
/// All data is lost when the process exits. This is intentional —
/// ephemeral mode is designed for:
/// - Privacy-critical demos (no disk traces)
/// - Travel mode (device seizure risk)
/// - Testing without SQLite
///
/// WARNING: This backend provides NO persistence. If the daemon crashes
/// or is killed, all rules, sessions, memory, and audit trails are gone.
pub struct InMemoryBackend {
    rules: RwLock<HashMap<String, Rule>>,
    connectors: RwLock<HashMap<String, ConnectorRow>>,
    events: RwLock<Vec<EventEntry>>,
    jobs: RwLock<Vec<JobRow>>,
    sessions: RwLock<HashMap<(String, String), SessionRow>>,
    user_prefs: RwLock<HashMap<String, UserPrefsRow>>,
    memory: RwLock<Vec<MemoryRow>>,
    aliases: RwLock<HashMap<String, (String, String)>>,
    audit: RwLock<Vec<AuditEntry>>,
    safety_config: RwLock<Option<SafetyConfigRow>>,
    formations: RwLock<Vec<FormationRow>>,
    formation_members: RwLock<Vec<FormationMemberRow>>,
}

impl InMemoryBackend {
    /// Create a new empty in-memory backend.
    pub fn new() -> Self {
        Self {
            rules: RwLock::new(HashMap::new()),
            connectors: RwLock::new(HashMap::new()),
            events: RwLock::new(Vec::new()),
            jobs: RwLock::new(Vec::new()),
            sessions: RwLock::new(HashMap::new()),
            user_prefs: RwLock::new(HashMap::new()),
            memory: RwLock::new(Vec::new()),
            aliases: RwLock::new(HashMap::new()),
            audit: RwLock::new(Vec::new()),
            safety_config: RwLock::new(None),
            formations: RwLock::new(Vec::new()),
            formation_members: RwLock::new(Vec::new()),
        }
    }
}

impl Default for InMemoryBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StorageBackend for InMemoryBackend {
    // ── Rules ──────────────────────────────────────────────────

    async fn insert_rule(&self, rule: &Rule) -> Result<RuleId, StoreError> {
        let id = rule.id;
        let mut rules = self.rules.write().await;
        rules.insert(id.to_string(), rule.clone());
        Ok(id)
    }

    async fn find_rules_by_trigger(&self, trigger_type: &str) -> Result<Vec<Rule>, StoreError> {
        let rules = self.rules.read().await;
        Ok(rules
            .values()
            .filter(|r| r.trigger.trigger_type() == trigger_type)
            .cloned()
            .collect())
    }

    async fn list_rules(&self) -> Result<Vec<Rule>, StoreError> {
        let rules = self.rules.read().await;
        Ok(rules.values().cloned().collect())
    }

    async fn toggle_rule(&self, id: &RuleId, enabled: bool) -> Result<(), StoreError> {
        let mut rules = self.rules.write().await;
        if let Some(rule) = rules.get_mut(&id.to_string()) {
            rule.status = if enabled {
                springtale_core::rule::types::RuleStatus::Enabled
            } else {
                springtale_core::rule::types::RuleStatus::Disabled
            };
            Ok(())
        } else {
            Err(StoreError::NotFound {
                entity: "rule".into(),
                id: id.to_string(),
            })
        }
    }

    async fn delete_rule(&self, id: &RuleId) -> Result<(), StoreError> {
        let mut rules = self.rules.write().await;
        rules.remove(&id.to_string());
        Ok(())
    }

    // ── Connectors ─────────────────────────────────────────────

    async fn register_connector(&self, row: &ConnectorRow) -> Result<(), StoreError> {
        let mut connectors = self.connectors.write().await;
        connectors.insert(row.name.clone(), row.clone());
        Ok(())
    }

    async fn list_connectors(&self) -> Result<Vec<ConnectorRow>, StoreError> {
        let connectors = self.connectors.read().await;
        Ok(connectors.values().cloned().collect())
    }

    async fn set_connector_enabled(&self, name: &str, enabled: bool) -> Result<(), StoreError> {
        let mut connectors = self.connectors.write().await;
        if let Some(c) = connectors.get_mut(name) {
            c.enabled = enabled;
            Ok(())
        } else {
            Err(StoreError::NotFound {
                entity: "connector".into(),
                id: name.to_owned(),
            })
        }
    }

    async fn remove_connector(&self, name: &str) -> Result<(), StoreError> {
        let mut connectors = self.connectors.write().await;
        connectors.remove(name);
        Ok(())
    }

    // ── Events ─────────────────────────────────────────────────

    async fn log_event(&self, event: &EventEntry) -> Result<(), StoreError> {
        let mut events = self.events.write().await;
        events.push(event.clone());
        Ok(())
    }

    async fn list_events(&self, filter: &EventFilter) -> Result<Vec<EventEntry>, StoreError> {
        let events = self.events.read().await;
        let filtered: Vec<EventEntry> = events
            .iter()
            .filter(|e| {
                if filter
                    .trigger_type
                    .as_ref()
                    .is_some_and(|tt| e.trigger_type != *tt)
                {
                    return false;
                }
                if filter.after.as_ref().is_some_and(|a| e.timestamp < *a) {
                    return false;
                }
                if filter.before.as_ref().is_some_and(|b| e.timestamp > *b) {
                    return false;
                }
                true
            })
            .take(filter.limit.unwrap_or(100) as usize)
            .cloned()
            .collect();
        Ok(filtered)
    }

    async fn delete_events_before(&self, before: &DateTime<Utc>) -> Result<u64, StoreError> {
        let mut events = self.events.write().await;
        let before_len = events.len();
        events.retain(|e| e.timestamp >= *before);
        Ok((before_len - events.len()) as u64)
    }

    // ── Jobs ───────────────────────────────────────────────────

    async fn enqueue_job(&self, job: &JobRow) -> Result<JobId, StoreError> {
        let mut jobs = self.jobs.write().await;
        jobs.push(job.clone());
        Ok(job.id)
    }

    async fn dequeue_job(&self) -> Result<Option<JobRow>, StoreError> {
        let mut jobs = self.jobs.write().await;
        if jobs.is_empty() {
            return Ok(None);
        }
        // Simple FIFO — remove first pending job
        let idx = jobs.iter().position(|j| j.status == "pending");
        if let Some(i) = idx {
            let mut job = jobs.remove(i);
            job.status = "running".to_owned();
            jobs.push(job.clone());
            Ok(Some(job))
        } else {
            Ok(None)
        }
    }

    async fn complete_job(&self, id: &JobId) -> Result<(), StoreError> {
        let mut jobs = self.jobs.write().await;
        if let Some(job) = jobs.iter_mut().find(|j| j.id == *id) {
            job.status = "completed".to_owned();
        }
        Ok(())
    }

    async fn fail_job(&self, id: &JobId, error: &str) -> Result<(), StoreError> {
        let mut jobs = self.jobs.write().await;
        if let Some(job) = jobs.iter_mut().find(|j| j.id == *id) {
            job.status = "failed".to_owned();
            job.last_error = Some(error.to_owned());
        }
        Ok(())
    }

    // ── Bot Sessions ──────────────────────────────────────────

    async fn upsert_session(&self, session: &SessionRow) -> Result<(), StoreError> {
        let mut sessions = self.sessions.write().await;
        let key = (session.user_id.clone(), session.channel_id.clone());
        sessions.insert(key, session.clone());
        Ok(())
    }

    async fn get_session(
        &self,
        user_id: &str,
        channel_id: &str,
    ) -> Result<Option<SessionRow>, StoreError> {
        let sessions = self.sessions.read().await;
        let key = (user_id.to_owned(), channel_id.to_owned());
        Ok(sessions.get(&key).cloned())
    }

    async fn delete_session(&self, user_id: &str, channel_id: &str) -> Result<(), StoreError> {
        let mut sessions = self.sessions.write().await;
        let key = (user_id.to_owned(), channel_id.to_owned());
        sessions.remove(&key);
        Ok(())
    }

    async fn list_sessions(&self) -> Result<Vec<SessionRow>, StoreError> {
        let sessions = self.sessions.read().await;
        Ok(sessions.values().cloned().collect())
    }

    // ── User Preferences ──────────────────────────────────────

    async fn upsert_user_prefs(&self, prefs: &UserPrefsRow) -> Result<(), StoreError> {
        let mut user_prefs = self.user_prefs.write().await;
        user_prefs.insert(prefs.user_id.clone(), prefs.clone());
        Ok(())
    }

    async fn get_user_prefs(&self, user_id: &str) -> Result<Option<UserPrefsRow>, StoreError> {
        let user_prefs = self.user_prefs.read().await;
        Ok(user_prefs.get(user_id).cloned())
    }

    // ── Bot Memory ────────────────────────────────────────────

    async fn insert_memory(&self, entry: &MemoryRow) -> Result<(), StoreError> {
        let mut memory = self.memory.write().await;
        memory.push(entry.clone());
        Ok(())
    }

    async fn get_memory(
        &self,
        user_id: &str,
        channel_id: &str,
        limit: usize,
    ) -> Result<Vec<MemoryRow>, StoreError> {
        let memory = self.memory.read().await;
        let mut matching: Vec<MemoryRow> = memory
            .iter()
            .filter(|m| m.user_id == user_id && m.channel_id == channel_id)
            .cloned()
            .collect();
        // Sort by created_at DESC (most recent first)
        matching.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        matching.truncate(limit);
        Ok(matching)
    }

    async fn delete_memory(&self, user_id: &str, channel_id: &str) -> Result<u64, StoreError> {
        let mut memory = self.memory.write().await;
        let before = memory.len();
        memory.retain(|m| !(m.user_id == user_id && m.channel_id == channel_id));
        Ok((before - memory.len()) as u64)
    }

    async fn compact_memory(
        &self,
        user_id: &str,
        channel_id: &str,
        max_entries: usize,
    ) -> Result<u64, StoreError> {
        let mut memory = self.memory.write().await;
        let mut matching: Vec<(usize, &MemoryRow)> = memory
            .iter()
            .enumerate()
            .filter(|(_, m)| m.user_id == user_id && m.channel_id == channel_id)
            .collect();
        // Sort by created_at DESC
        matching.sort_by(|a, b| b.1.created_at.cmp(&a.1.created_at));

        if matching.len() <= max_entries {
            return Ok(0);
        }

        // Indices to remove (oldest beyond max_entries)
        let to_remove: Vec<usize> = matching[max_entries..].iter().map(|(i, _)| *i).collect();
        let count = to_remove.len() as u64;

        // Remove in reverse index order to avoid shifting
        let mut to_remove_sorted = to_remove;
        to_remove_sorted.sort_unstable();
        for idx in to_remove_sorted.into_iter().rev() {
            memory.remove(idx);
        }

        Ok(count)
    }

    // ── Bot Aliases ───────────────────────────────────────────

    async fn upsert_alias(
        &self,
        alias: &str,
        target: &str,
        created_by: &str,
    ) -> Result<(), StoreError> {
        let mut aliases = self.aliases.write().await;
        aliases.insert(alias.to_owned(), (target.to_owned(), created_by.to_owned()));
        Ok(())
    }

    async fn list_aliases(&self) -> Result<Vec<(String, String)>, StoreError> {
        let aliases = self.aliases.read().await;
        Ok(aliases
            .iter()
            .map(|(alias, (target, _))| (alias.clone(), target.clone()))
            .collect())
    }

    async fn delete_alias(&self, alias: &str) -> Result<(), StoreError> {
        let mut aliases = self.aliases.write().await;
        aliases.remove(alias);
        Ok(())
    }

    // ── Audit Trail ───────────────────────────────────────────

    async fn insert_audit_entry(&self, entry: &AuditEntry) -> Result<(), StoreError> {
        let mut audit = self.audit.write().await;
        audit.push(entry.clone());
        Ok(())
    }

    async fn list_audit_entries(
        &self,
        filter: &AuditFilter,
    ) -> Result<Vec<AuditEntry>, StoreError> {
        let audit = self.audit.read().await;
        let filtered: Vec<AuditEntry> = audit
            .iter()
            .filter(|e| {
                if filter
                    .connector_name
                    .as_ref()
                    .is_some_and(|c| e.connector_name != *c)
                {
                    return false;
                }
                if filter.after.as_ref().is_some_and(|a| e.timestamp < *a) {
                    return false;
                }
                if filter.before.as_ref().is_some_and(|b| e.timestamp > *b) {
                    return false;
                }
                true
            })
            .take(filter.limit.unwrap_or(100) as usize)
            .cloned()
            .collect();
        Ok(filtered)
    }

    async fn export_audit(
        &self,
        after: &DateTime<Utc>,
        before: &DateTime<Utc>,
    ) -> Result<Vec<AuditEntry>, StoreError> {
        let audit = self.audit.read().await;
        Ok(audit
            .iter()
            .filter(|e| e.timestamp >= *after && e.timestamp <= *before)
            .cloned()
            .collect())
    }

    async fn delete_audit_before(&self, before: &DateTime<Utc>) -> Result<u64, StoreError> {
        let mut audit = self.audit.write().await;
        let before_len = audit.len();
        audit.retain(|e| e.timestamp >= *before);
        Ok((before_len - audit.len()) as u64)
    }

    /// No-op for in-memory backend — data is already ephemeral.
    /// All state lives in HashMaps and is lost on process exit.
    async fn get_safety_config(&self) -> Result<Option<SafetyConfigRow>, StoreError> {
        Ok(self.safety_config.read().await.clone())
    }

    async fn upsert_safety_config(&self, config: &SafetyConfigRow) -> Result<(), StoreError> {
        *self.safety_config.write().await = Some(config.clone());
        Ok(())
    }

    async fn insert_formation(&self, row: &FormationRow) -> Result<(), StoreError> {
        self.formations.write().await.push(row.clone());
        Ok(())
    }

    async fn list_formations(&self) -> Result<Vec<FormationRow>, StoreError> {
        Ok(self.formations.read().await.clone())
    }

    async fn get_formation(&self, id: &str) -> Result<Option<FormationRow>, StoreError> {
        Ok(self.formations.read().await.iter().find(|f| f.id == id).cloned())
    }

    async fn update_formation_status(&self, id: &str, status: &str) -> Result<(), StoreError> {
        let mut formations = self.formations.write().await;
        if let Some(f) = formations.iter_mut().find(|f| f.id == id) {
            f.status = status.to_owned();
            f.updated_at = chrono::Utc::now();
        }
        Ok(())
    }

    async fn delete_formation(&self, id: &str) -> Result<(), StoreError> {
        self.formations.write().await.retain(|f| f.id != id);
        self.formation_members.write().await.retain(|m| m.formation_id != id);
        Ok(())
    }

    async fn insert_formation_member(&self, row: &FormationMemberRow) -> Result<(), StoreError> {
        self.formation_members.write().await.push(row.clone());
        Ok(())
    }

    async fn list_formation_members(&self, formation_id: &str) -> Result<Vec<FormationMemberRow>, StoreError> {
        Ok(self.formation_members.read().await.iter().filter(|m| m.formation_id == formation_id).cloned().collect())
    }

    fn panic_wipe(&self) -> Result<(), StoreError> {
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_insert_and_list_rules() {
        let backend = InMemoryBackend::new();
        let rules = backend.list_rules().await.unwrap();
        assert!(rules.is_empty());
    }

    #[tokio::test]
    async fn test_session_upsert_and_get() {
        let backend = InMemoryBackend::new();
        let session = SessionRow {
            user_id: "U123".to_owned(),
            channel_id: "C456".to_owned(),
            last_bot_message: None,
            pending_command: None,
            state_data: "{}".to_owned(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        backend.upsert_session(&session).await.unwrap();
        let retrieved = backend.get_session("U123", "C456").await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().user_id, "U123");
    }

    #[tokio::test]
    async fn test_session_delete() {
        let backend = InMemoryBackend::new();
        let session = SessionRow {
            user_id: "U123".to_owned(),
            channel_id: "C456".to_owned(),
            last_bot_message: None,
            pending_command: None,
            state_data: "{}".to_owned(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        backend.upsert_session(&session).await.unwrap();
        backend.delete_session("U123", "C456").await.unwrap();
        let retrieved = backend.get_session("U123", "C456").await.unwrap();
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_alias_crud() {
        let backend = InMemoryBackend::new();
        backend.upsert_alias("hi", "help", "user1").await.unwrap();
        let aliases = backend.list_aliases().await.unwrap();
        assert_eq!(aliases.len(), 1);
        assert_eq!(aliases[0].0, "hi");
        assert_eq!(aliases[0].1, "help");

        backend.delete_alias("hi").await.unwrap();
        let aliases = backend.list_aliases().await.unwrap();
        assert!(aliases.is_empty());
    }

    #[tokio::test]
    async fn test_connector_register_and_list() {
        let backend = InMemoryBackend::new();
        let row = ConnectorRow {
            name: "test-connector".to_owned(),
            version: "0.1.0".to_owned(),
            author: "test".to_owned(),
            description: "test connector".to_owned(),
            manifest_json: "{}".to_owned(),
            enabled: true,
            installed_at: Utc::now(),
        };
        backend.register_connector(&row).await.unwrap();
        let connectors = backend.list_connectors().await.unwrap();
        assert_eq!(connectors.len(), 1);
        assert_eq!(connectors[0].name, "test-connector");
    }
}
