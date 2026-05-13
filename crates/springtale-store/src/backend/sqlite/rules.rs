use chrono::Utc;
use rusqlite::params;

use crate::error::StoreError;
use springtale_core::rule::types::{Rule, RuleId, RuleOwner};

use super::SqliteBackend;

impl SqliteBackend {
    pub(super) async fn insert_rule_impl(&self, rule: &Rule) -> Result<RuleId, StoreError> {
        let conn = self.conn.clone();
        let rule = rule.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            let rule_toml =
                toml::to_string(&rule).map_err(|e| StoreError::Serialization(e.to_string()))?;
            let now = Utc::now().to_rfc3339();
            let (owner_kind, owner_agent_id, owner_formation_id) = owner_columns(&rule.owner);

            conn.execute(
                "INSERT INTO rules (id, name, description, status, version, trigger_type, rule_toml, owner_kind, owner_agent_id, owner_formation_id, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    rule.id.0.to_string(),
                    rule.name,
                    rule.description,
                    format!("{:?}", rule.status).to_lowercase(),
                    rule.version.0 as i64,
                    rule.trigger.trigger_type(),
                    rule_toml,
                    owner_kind,
                    owner_agent_id,
                    owner_formation_id,
                    now,
                    now,
                ],
            )?;

            Ok(rule.id)
        })
        .await
        .map_err(|e| StoreError::Database(format!("spawn_blocking join: {e}")))?
    }

    pub(super) async fn find_rules_by_trigger_impl(
        &self,
        trigger_type: &str,
    ) -> Result<Vec<Rule>, StoreError> {
        let conn = self.conn.clone();
        let trigger_type = trigger_type.to_owned();
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            let mut stmt = conn.prepare(
                "SELECT rule_toml FROM rules WHERE trigger_type = ?1 AND status = 'enabled'",
            )?;
            let rows = stmt
                .query_map(params![trigger_type], |row| {
                    let toml_str: String = row.get(0)?;
                    Ok(toml_str)
                })?
                .collect::<Result<Vec<String>, _>>()?;

            let mut rules = Vec::new();
            for toml_str in rows {
                let rule: Rule = toml::from_str(&toml_str)
                    .map_err(|e| StoreError::Serialization(e.to_string()))?;
                rules.push(rule);
            }
            Ok(rules)
        })
        .await
        .map_err(|e| StoreError::Database(format!("spawn_blocking join: {e}")))?
    }

    pub(super) async fn list_rules_impl(&self) -> Result<Vec<Rule>, StoreError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            let mut stmt = conn.prepare("SELECT rule_toml FROM rules ORDER BY created_at")?;
            let rows = stmt
                .query_map([], |row| {
                    let toml_str: String = row.get(0)?;
                    Ok(toml_str)
                })?
                .collect::<Result<Vec<String>, _>>()?;

            let mut rules = Vec::new();
            for toml_str in rows {
                let rule: Rule = toml::from_str(&toml_str)
                    .map_err(|e| StoreError::Serialization(e.to_string()))?;
                rules.push(rule);
            }
            Ok(rules)
        })
        .await
        .map_err(|e| StoreError::Database(format!("spawn_blocking join: {e}")))?
    }

    pub(super) async fn toggle_rule_impl(
        &self,
        id: &RuleId,
        enabled: bool,
    ) -> Result<(), StoreError> {
        let conn = self.conn.clone();
        let id = *id;
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            let status = if enabled { "enabled" } else { "disabled" };
            let updated = conn.execute(
                "UPDATE rules SET status = ?1, updated_at = ?2 WHERE id = ?3",
                params![status, Utc::now().to_rfc3339(), id.0.to_string()],
            )?;
            if updated == 0 {
                return Err(StoreError::NotFound {
                    entity: "rule".into(),
                    id: id.to_string(),
                });
            }
            Ok(())
        })
        .await
        .map_err(|e| StoreError::Database(format!("spawn_blocking join: {e}")))?
    }

    pub(super) async fn set_rule_activation_error_impl(
        &self,
        id: &RuleId,
        error: Option<&str>,
    ) -> Result<(), StoreError> {
        let conn = self.conn.clone();
        let id = *id;
        let error = error.map(|s| s.to_owned());
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            conn.execute(
                "UPDATE rules SET activation_error = ?1 WHERE id = ?2",
                params![error, id.0.to_string()],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| StoreError::Database(format!("spawn_blocking join: {e}")))?
    }

    pub(super) async fn get_rule_activation_errors_impl(
        &self,
    ) -> Result<std::collections::HashMap<String, String>, StoreError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            let mut stmt = conn.prepare(
                "SELECT id, activation_error FROM rules WHERE activation_error IS NOT NULL",
            )?;
            let rows = stmt
                .query_map([], |row| {
                    let id: String = row.get(0)?;
                    let error: String = row.get(1)?;
                    Ok((id, error))
                })?
                .collect::<Result<Vec<(String, String)>, _>>()?;
            Ok(rows.into_iter().collect())
        })
        .await
        .map_err(|e| StoreError::Database(format!("spawn_blocking join: {e}")))?
    }

    pub(super) async fn delete_rule_impl(&self, id: &RuleId) -> Result<(), StoreError> {
        let conn = self.conn.clone();
        let id = *id;
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            let deleted =
                conn.execute("DELETE FROM rules WHERE id = ?1", params![id.0.to_string()])?;
            if deleted == 0 {
                return Err(StoreError::NotFound {
                    entity: "rule".into(),
                    id: id.to_string(),
                });
            }
            Ok(())
        })
        .await
        .map_err(|e| StoreError::Database(format!("spawn_blocking join: {e}")))?
    }
}

/// Project a [`RuleOwner`] into the denormalized SQL columns
/// `(owner_kind, owner_agent_id, owner_formation_id)`. The TOML
/// payload still carries the full enum — these columns are the index
/// for "list rules owned by X" admin queries and (Phase B) executions
/// log joins.
fn owner_columns(owner: &RuleOwner) -> (&'static str, Option<String>, Option<String>) {
    match owner {
        RuleOwner::Global => ("global", None, None),
        RuleOwner::Agent { agent_id } => ("agent", Some(agent_id.to_string()), None),
        RuleOwner::Formation { formation_id } => {
            ("formation", None, Some(formation_id.to_string()))
        }
    }
}
