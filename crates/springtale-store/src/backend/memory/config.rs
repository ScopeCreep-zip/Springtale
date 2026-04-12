use crate::error::StoreError;

use super::InMemoryBackend;

impl InMemoryBackend {
    pub(super) async fn get_config_impl(&self, key: &str) -> Result<Option<String>, StoreError> {
        Ok(self.config.read().await.get(key).cloned())
    }

    pub(super) async fn set_config_impl(
        &self,
        key: &str,
        value_json: &str,
    ) -> Result<(), StoreError> {
        self.config
            .write()
            .await
            .insert(key.to_owned(), value_json.to_owned());
        Ok(())
    }

    pub(super) async fn list_config_impl(&self) -> Result<Vec<(String, String)>, StoreError> {
        let map = self.config.read().await;
        let mut entries: Vec<_> = map.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(entries)
    }
}
