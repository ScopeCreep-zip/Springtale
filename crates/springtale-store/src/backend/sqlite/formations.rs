use chrono::Utc;
use rusqlite::params;

use crate::error::StoreError;
use crate::schema::formations::{FormationMemberRow, FormationRow};

use super::SqliteBackend;

impl SqliteBackend {
    pub(super) async fn insert_formation_impl(
        &self,
        row: &FormationRow,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT INTO formations (id, name, intent, status, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                row.id,
                row.name,
                row.intent,
                row.status,
                row.created_at.to_rfc3339(),
                row.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub(super) async fn list_formations_impl(&self) -> Result<Vec<FormationRow>, StoreError> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, name, intent, status, created_at, updated_at FROM formations ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?;

        let mut formations = Vec::new();
        for row in rows {
            let (id, name, intent, status, created, updated) = row?;
            let created_at = chrono::DateTime::parse_from_rfc3339(&created)
                .map(|dt| dt.with_timezone(&Utc))
                .map_err(|e| StoreError::Serialization(e.to_string()))?;
            let updated_at = chrono::DateTime::parse_from_rfc3339(&updated)
                .map(|dt| dt.with_timezone(&Utc))
                .map_err(|e| StoreError::Serialization(e.to_string()))?;
            formations.push(FormationRow { id, name, intent, status, created_at, updated_at });
        }
        Ok(formations)
    }

    pub(super) async fn get_formation_impl(
        &self,
        id: &str,
    ) -> Result<Option<FormationRow>, StoreError> {
        let conn = self.conn.lock().await;
        let result = conn.query_row(
            "SELECT id, name, intent, status, created_at, updated_at FROM formations WHERE id = ?1",
            params![id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        );

        match result {
            Ok((fid, name, intent, status, created, updated)) => {
                let created_at = chrono::DateTime::parse_from_rfc3339(&created)
                    .map(|dt| dt.with_timezone(&Utc))
                    .map_err(|e| StoreError::Serialization(e.to_string()))?;
                let updated_at = chrono::DateTime::parse_from_rfc3339(&updated)
                    .map(|dt| dt.with_timezone(&Utc))
                    .map_err(|e| StoreError::Serialization(e.to_string()))?;
                Ok(Some(FormationRow { id: fid, name, intent, status, created_at, updated_at }))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub(super) async fn update_formation_status_impl(
        &self,
        id: &str,
        status: &str,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().await;
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE formations SET status = ?1, updated_at = ?2 WHERE id = ?3",
            params![status, now, id],
        )?;
        Ok(())
    }

    pub(super) async fn update_formation_intent_impl(
        &self,
        id: &str,
        intent: &str,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().await;
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE formations SET intent = ?1, updated_at = ?2 WHERE id = ?3",
            params![intent, now, id],
        )?;
        Ok(())
    }

    pub(super) async fn delete_formation_impl(&self, id: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock().await;
        conn.execute("DELETE FROM formation_members WHERE formation_id = ?1", params![id])?;
        conn.execute("DELETE FROM formations WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub(super) async fn insert_formation_member_impl(
        &self,
        row: &FormationMemberRow,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT INTO formation_members (id, formation_id, connector_name, role_hint)
             VALUES (?1, ?2, ?3, ?4)",
            params![row.id, row.formation_id, row.connector_name, row.role_hint],
        )?;
        Ok(())
    }

    pub(super) async fn list_formation_members_impl(
        &self,
        formation_id: &str,
    ) -> Result<Vec<FormationMemberRow>, StoreError> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, formation_id, connector_name, role_hint FROM formation_members WHERE formation_id = ?1",
        )?;
        let rows = stmt.query_map(params![formation_id], |row| {
            Ok(FormationMemberRow {
                id: row.get(0)?,
                formation_id: row.get(1)?,
                connector_name: row.get(2)?,
                role_hint: row.get(3)?,
            })
        })?;

        let mut members = Vec::new();
        for row in rows {
            members.push(row?);
        }
        Ok(members)
    }
}
