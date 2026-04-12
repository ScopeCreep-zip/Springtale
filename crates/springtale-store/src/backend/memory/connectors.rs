use crate::error::StoreError;
use crate::schema::connectors::ConnectorRow;

use super::InMemoryBackend;

impl InMemoryBackend {
    pub(super) async fn register_connector_impl(
        &self,
        row: &ConnectorRow,
    ) -> Result<(), StoreError> {
        let mut connectors = self.connectors.write().await;
        connectors.insert(row.name.clone(), row.clone());
        Ok(())
    }

    pub(super) async fn list_connectors_impl(&self) -> Result<Vec<ConnectorRow>, StoreError> {
        let connectors = self.connectors.read().await;
        Ok(connectors.values().cloned().collect())
    }

    pub(super) async fn set_connector_enabled_impl(
        &self,
        name: &str,
        enabled: bool,
    ) -> Result<(), StoreError> {
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

    pub(super) async fn remove_connector_impl(&self, name: &str) -> Result<(), StoreError> {
        let mut connectors = self.connectors.write().await;
        connectors.remove(name);
        Ok(())
    }
}
