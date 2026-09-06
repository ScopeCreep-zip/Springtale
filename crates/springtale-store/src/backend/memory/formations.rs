use crate::error::StoreError;
use crate::schema::formations::{FormationMemberRow, FormationRow};

use super::InMemoryBackend;

impl InMemoryBackend {
    pub(super) async fn insert_formation_impl(&self, row: &FormationRow) -> Result<(), StoreError> {
        self.formations.write().await.push(row.clone());
        Ok(())
    }

    pub(super) async fn list_formations_impl(&self) -> Result<Vec<FormationRow>, StoreError> {
        Ok(self.formations.read().await.clone())
    }

    pub(super) async fn get_formation_impl(
        &self,
        id: &str,
    ) -> Result<Option<FormationRow>, StoreError> {
        Ok(self
            .formations
            .read()
            .await
            .iter()
            .find(|f| f.id == id)
            .cloned())
    }

    pub(super) async fn update_formation_status_impl(
        &self,
        id: &str,
        status: &str,
    ) -> Result<(), StoreError> {
        let mut formations = self.formations.write().await;
        if let Some(f) = formations.iter_mut().find(|f| f.id == id) {
            f.status = status.to_owned();
            f.updated_at = chrono::Utc::now();
        }
        Ok(())
    }

    pub(super) async fn update_formation_intent_impl(
        &self,
        id: &str,
        intent: &str,
    ) -> Result<(), StoreError> {
        let mut formations = self.formations.write().await;
        if let Some(f) = formations.iter_mut().find(|f| f.id == id) {
            f.intent = intent.to_owned();
            f.updated_at = chrono::Utc::now();
        }
        Ok(())
    }

    pub(super) async fn delete_formation_impl(&self, id: &str) -> Result<(), StoreError> {
        self.formations.write().await.retain(|f| f.id != id);
        self.formation_members
            .write()
            .await
            .retain(|m| m.formation_id != id);
        Ok(())
    }

    pub(super) async fn insert_formation_member_impl(
        &self,
        row: &FormationMemberRow,
    ) -> Result<(), StoreError> {
        // One member per connector per formation, matching the unique index
        // the SQLite schema declares: a member-owned rule finds its member
        // through the connector name, so a second row would make that
        // binding ambiguous.
        let mut members = self.formation_members.write().await;
        if members
            .iter()
            .any(|m| m.formation_id == row.formation_id && m.connector_name == row.connector_name)
        {
            return Ok(());
        }
        members.push(row.clone());
        Ok(())
    }

    pub(super) async fn list_formation_members_impl(
        &self,
        formation_id: &str,
    ) -> Result<Vec<FormationMemberRow>, StoreError> {
        Ok(self
            .formation_members
            .read()
            .await
            .iter()
            .filter(|m| m.formation_id == formation_id)
            .cloned()
            .collect())
    }

    pub(super) async fn delete_formation_member_impl(
        &self,
        formation_id: &str,
        connector_name: &str,
    ) -> Result<(), StoreError> {
        self.formation_members
            .write()
            .await
            .retain(|m| !(m.formation_id == formation_id && m.connector_name == connector_name));
        Ok(())
    }
}
