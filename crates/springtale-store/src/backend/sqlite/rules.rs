use chrono::Utc;
use rusqlite::params;

use crate::error::StoreError;
use springtale_core::rule::types::{Rule, RuleId};

use super::SqliteBackend;

impl SqliteBackend {
    pub(super) async fn insert_rule_impl(&self, rule: &Rule) -> Result<RuleId, StoreError> {
        let conn = self.conn.lock().await;
        let rule_toml =
            toml::to_string(rule).map_err(|e| StoreError::Serialization(e.to_string()))?;
        let now = Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO rules (id, name, description, status, version, trigger_type, rule_toml, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                rule.id.0.to_string(),
                rule.name,
                rule.description,
                format!("{:?}", rule.status).to_lowercase(),
                rule.version.0 as i64,
                rule.trigger.trigger_type(),
                rule_toml,
                now,
                now,
            ],
        )?;

        Ok(rule.id)
    }

    pub(super) async fn find_rules_by_trigger_impl(
        &self,
        trigger_type: &str,
    ) -> Result<Vec<Rule>, StoreError> {
        let conn = self.conn.lock().await;
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
            let rule: Rule =
                toml::from_str(&toml_str).map_err(|e| StoreError::Serialization(e.to_string()))?;
            rules.push(rule);
        }
        Ok(rules)
    }

    pub(super) async fn list_rules_impl(&self) -> Result<Vec<Rule>, StoreError> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare("SELECT rule_toml FROM rules ORDER BY created_at")?;
        let rows = stmt
            .query_map([], |row| {
                let toml_str: String = row.get(0)?;
                Ok(toml_str)
            })?
            .collect::<Result<Vec<String>, _>>()?;

        let mut rules = Vec::new();
        for toml_str in rows {
            let rule: Rule =
                toml::from_str(&toml_str).map_err(|e| StoreError::Serialization(e.to_string()))?;
            rules.push(rule);
        }
        Ok(rules)
    }

    pub(super) async fn toggle_rule_impl(
        &self,
        id: &RuleId,
        enabled: bool,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().await;
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
    }

    pub(super) async fn delete_rule_impl(&self, id: &RuleId) -> Result<(), StoreError> {
        let conn = self.conn.lock().await;
        let deleted = conn.execute("DELETE FROM rules WHERE id = ?1", params![id.0.to_string()])?;
        if deleted == 0 {
            return Err(StoreError::NotFound {
                entity: "rule".into(),
                id: id.to_string(),
            });
        }
        Ok(())
    }
}
