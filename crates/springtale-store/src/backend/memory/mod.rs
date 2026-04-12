mod aliases;
mod audit;
mod config;
mod connectors;
mod events;
mod formations;
mod jobs;
mod rules;
mod safety;
mod sessions;

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tokio::sync::RwLock;

use crate::error::StoreError;
use crate::schema::audit::{AuditEntry, AuditFilter};
use crate::schema::bot::{MemoryRow, SessionRow, UserPrefsRow};
use crate::schema::connectors::ConnectorRow;
use crate::schema::events::{EventEntry, EventFilter};
use crate::schema::formations::{FormationMemberRow, FormationRow};
use crate::schema::jobs::{JobId, JobRow};
use crate::schema::safety::SafetyConfigRow;
use springtale_core::rule::types::{Rule, RuleId};

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
    config: RwLock<HashMap<String, String>>,
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
            config: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl super::trait_::StorageBackend for InMemoryBackend {
    // ── Rules ──────────────────────────────────────────────────

    async fn insert_rule(&self, rule: &Rule) -> Result<RuleId, StoreError> {
        self.insert_rule_impl(rule).await
    }

    async fn find_rules_by_trigger(&self, trigger_type: &str) -> Result<Vec<Rule>, StoreError> {
        self.find_rules_by_trigger_impl(trigger_type).await
    }

    async fn list_rules(&self) -> Result<Vec<Rule>, StoreError> {
        self.list_rules_impl().await
    }

    async fn toggle_rule(&self, id: &RuleId, enabled: bool) -> Result<(), StoreError> {
        self.toggle_rule_impl(id, enabled).await
    }

    async fn delete_rule(&self, id: &RuleId) -> Result<(), StoreError> {
        self.delete_rule_impl(id).await
    }

    async fn set_rule_activation_error(
        &self,
        _id: &RuleId,
        _error: Option<&str>,
    ) -> Result<(), StoreError> {
        Ok(())
    }

    async fn get_rule_activation_errors(
        &self,
    ) -> Result<std::collections::HashMap<String, String>, StoreError> {
        // In-memory backend doesn't track activation errors.
        Ok(std::collections::HashMap::new())
    }

    // ── Connectors ─────────────────────────────────────────────

    async fn register_connector(&self, row: &ConnectorRow) -> Result<(), StoreError> {
        self.register_connector_impl(row).await
    }

    async fn list_connectors(&self) -> Result<Vec<ConnectorRow>, StoreError> {
        self.list_connectors_impl().await
    }

    async fn set_connector_enabled(&self, name: &str, enabled: bool) -> Result<(), StoreError> {
        self.set_connector_enabled_impl(name, enabled).await
    }

    async fn remove_connector(&self, name: &str) -> Result<(), StoreError> {
        self.remove_connector_impl(name).await
    }

    // ── Events ─────────────────────────────────────────────────

    async fn log_event(&self, event: &EventEntry) -> Result<(), StoreError> {
        self.log_event_impl(event).await
    }

    async fn list_events(&self, filter: &EventFilter) -> Result<Vec<EventEntry>, StoreError> {
        self.list_events_impl(filter).await
    }

    async fn delete_events_before(&self, before: &DateTime<Utc>) -> Result<u64, StoreError> {
        self.delete_events_before_impl(before).await
    }

    // ── Jobs ───────────────────────────────────────────────────

    async fn enqueue_job(&self, job: &JobRow) -> Result<JobId, StoreError> {
        self.enqueue_job_impl(job).await
    }

    async fn dequeue_job(&self) -> Result<Option<JobRow>, StoreError> {
        self.dequeue_job_impl().await
    }

    async fn complete_job(&self, id: &JobId) -> Result<(), StoreError> {
        self.complete_job_impl(id).await
    }

    async fn fail_job(&self, id: &JobId, error: &str) -> Result<(), StoreError> {
        self.fail_job_impl(id, error).await
    }

    // ── Bot Sessions ──────────────────────────────────────────

    async fn upsert_session(&self, session: &SessionRow) -> Result<(), StoreError> {
        self.upsert_session_impl(session).await
    }

    async fn get_session(
        &self,
        user_id: &str,
        channel_id: &str,
    ) -> Result<Option<SessionRow>, StoreError> {
        self.get_session_impl(user_id, channel_id).await
    }

    async fn delete_session(&self, user_id: &str, channel_id: &str) -> Result<(), StoreError> {
        self.delete_session_impl(user_id, channel_id).await
    }

    async fn list_sessions(&self) -> Result<Vec<SessionRow>, StoreError> {
        self.list_sessions_impl().await
    }

    // ── User Preferences ──────────────────────────────────────

    async fn upsert_user_prefs(&self, prefs: &UserPrefsRow) -> Result<(), StoreError> {
        self.upsert_user_prefs_impl(prefs).await
    }

    async fn get_user_prefs(&self, user_id: &str) -> Result<Option<UserPrefsRow>, StoreError> {
        self.get_user_prefs_impl(user_id).await
    }

    // ── Bot Memory ────────────────────────────────────────────

    async fn insert_memory(&self, entry: &MemoryRow) -> Result<(), StoreError> {
        self.insert_memory_impl(entry).await
    }

    async fn get_memory(
        &self,
        user_id: &str,
        channel_id: &str,
        limit: usize,
    ) -> Result<Vec<MemoryRow>, StoreError> {
        self.get_memory_impl(user_id, channel_id, limit).await
    }

    async fn delete_memory(&self, user_id: &str, channel_id: &str) -> Result<u64, StoreError> {
        self.delete_memory_impl(user_id, channel_id).await
    }

    async fn compact_memory(
        &self,
        user_id: &str,
        channel_id: &str,
        max_entries: usize,
    ) -> Result<u64, StoreError> {
        self.compact_memory_impl(user_id, channel_id, max_entries)
            .await
    }

    // ── Bot Aliases ───────────────────────────────────────────

    async fn upsert_alias(
        &self,
        alias: &str,
        target: &str,
        created_by: &str,
    ) -> Result<(), StoreError> {
        self.upsert_alias_impl(alias, target, created_by).await
    }

    async fn list_aliases(&self) -> Result<Vec<(String, String)>, StoreError> {
        self.list_aliases_impl().await
    }

    async fn delete_alias(&self, alias: &str) -> Result<(), StoreError> {
        self.delete_alias_impl(alias).await
    }

    // ── Audit Trail ───────────────────────────────────────────

    async fn insert_audit_entry(&self, entry: &AuditEntry) -> Result<(), StoreError> {
        self.insert_audit_entry_impl(entry).await
    }

    async fn list_audit_entries(
        &self,
        filter: &AuditFilter,
    ) -> Result<Vec<AuditEntry>, StoreError> {
        self.list_audit_entries_impl(filter).await
    }

    async fn export_audit(
        &self,
        after: &DateTime<Utc>,
        before: &DateTime<Utc>,
    ) -> Result<Vec<AuditEntry>, StoreError> {
        self.export_audit_impl(after, before).await
    }

    async fn delete_audit_before(&self, before: &DateTime<Utc>) -> Result<u64, StoreError> {
        self.delete_audit_before_impl(before).await
    }

    // ── Safety Config ──────────────────────────────────────────

    async fn get_safety_config(&self) -> Result<Option<SafetyConfigRow>, StoreError> {
        self.get_safety_config_impl().await
    }

    async fn upsert_safety_config(&self, config: &SafetyConfigRow) -> Result<(), StoreError> {
        self.upsert_safety_config_impl(config).await
    }

    // ── Formations ────────────────────────────────────────────

    async fn insert_formation(&self, row: &FormationRow) -> Result<(), StoreError> {
        self.insert_formation_impl(row).await
    }

    async fn list_formations(&self) -> Result<Vec<FormationRow>, StoreError> {
        self.list_formations_impl().await
    }

    async fn get_formation(&self, id: &str) -> Result<Option<FormationRow>, StoreError> {
        self.get_formation_impl(id).await
    }

    async fn update_formation_status(&self, id: &str, status: &str) -> Result<(), StoreError> {
        self.update_formation_status_impl(id, status).await
    }

    async fn update_formation_intent(&self, id: &str, intent: &str) -> Result<(), StoreError> {
        self.update_formation_intent_impl(id, intent).await
    }

    async fn delete_formation(&self, id: &str) -> Result<(), StoreError> {
        self.delete_formation_impl(id).await
    }

    async fn insert_formation_member(&self, row: &FormationMemberRow) -> Result<(), StoreError> {
        self.insert_formation_member_impl(row).await
    }

    async fn list_formation_members(
        &self,
        formation_id: &str,
    ) -> Result<Vec<FormationMemberRow>, StoreError> {
        self.list_formation_members_impl(formation_id).await
    }

    // ── Config Store ────────────────────────────────────────────

    async fn get_config(&self, key: &str) -> Result<Option<String>, StoreError> {
        self.get_config_impl(key).await
    }

    async fn set_config(&self, key: &str, value_json: &str) -> Result<(), StoreError> {
        self.set_config_impl(key, value_json).await
    }

    async fn list_config(&self) -> Result<Vec<(String, String)>, StoreError> {
        self.list_config_impl().await
    }

    // ── Emergency ─────────────────────────────────────────────

    /// No-op for in-memory backend — data is already ephemeral.
    /// All state lives in HashMaps and is lost on process exit.
    fn panic_wipe(&self) -> Result<(), StoreError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
