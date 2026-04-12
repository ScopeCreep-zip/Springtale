use crate::error::StoreError;

use super::InMemoryBackend;

impl InMemoryBackend {
    pub(super) async fn upsert_alias_impl(
        &self,
        alias: &str,
        target: &str,
        created_by: &str,
    ) -> Result<(), StoreError> {
        let mut aliases = self.aliases.write().await;
        aliases.insert(alias.to_owned(), (target.to_owned(), created_by.to_owned()));
        Ok(())
    }

    pub(super) async fn list_aliases_impl(&self) -> Result<Vec<(String, String)>, StoreError> {
        let aliases = self.aliases.read().await;
        Ok(aliases
            .iter()
            .map(|(alias, (target, _))| (alias.clone(), target.clone()))
            .collect())
    }

    pub(super) async fn delete_alias_impl(&self, alias: &str) -> Result<(), StoreError> {
        let mut aliases = self.aliases.write().await;
        aliases.remove(alias);
        Ok(())
    }
}
