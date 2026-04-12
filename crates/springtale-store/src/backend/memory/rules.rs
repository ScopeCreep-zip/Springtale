use crate::error::StoreError;
use springtale_core::rule::types::{Rule, RuleId};

use super::InMemoryBackend;

impl InMemoryBackend {
    pub(super) async fn insert_rule_impl(&self, rule: &Rule) -> Result<RuleId, StoreError> {
        let id = rule.id;
        let mut rules = self.rules.write().await;
        rules.insert(id.to_string(), rule.clone());
        Ok(id)
    }

    pub(super) async fn find_rules_by_trigger_impl(
        &self,
        trigger_type: &str,
    ) -> Result<Vec<Rule>, StoreError> {
        let rules = self.rules.read().await;
        Ok(rules
            .values()
            .filter(|r| r.trigger.trigger_type() == trigger_type)
            .cloned()
            .collect())
    }

    pub(super) async fn list_rules_impl(&self) -> Result<Vec<Rule>, StoreError> {
        let rules = self.rules.read().await;
        Ok(rules.values().cloned().collect())
    }

    pub(super) async fn toggle_rule_impl(
        &self,
        id: &RuleId,
        enabled: bool,
    ) -> Result<(), StoreError> {
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

    pub(super) async fn delete_rule_impl(&self, id: &RuleId) -> Result<(), StoreError> {
        let mut rules = self.rules.write().await;
        rules.remove(&id.to_string());
        Ok(())
    }
}
