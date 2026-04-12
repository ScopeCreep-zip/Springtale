use crate::error::StoreError;
use crate::schema::safety::SafetyConfigRow;

use super::InMemoryBackend;

impl InMemoryBackend {
    pub(super) async fn get_safety_config_impl(
        &self,
    ) -> Result<Option<SafetyConfigRow>, StoreError> {
        Ok(self.safety_config.read().await.clone())
    }

    pub(super) async fn upsert_safety_config_impl(
        &self,
        config: &SafetyConfigRow,
    ) -> Result<(), StoreError> {
        *self.safety_config.write().await = Some(config.clone());
        Ok(())
    }
}
