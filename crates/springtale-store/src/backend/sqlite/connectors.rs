use chrono::Utc;
use rusqlite::params;

use crate::error::StoreError;
use crate::schema::connectors::ConnectorRow;

use super::SqliteBackend;

impl SqliteBackend {
    pub(super) async fn register_connector_impl(
        &self,
        row: &ConnectorRow,
    ) -> Result<(), StoreError> {
        let conn = self.conn.clone();
        let row = row.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            conn.execute(
                "INSERT OR REPLACE INTO connectors (name, version, author, description, manifest_json, enabled, installed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    row.name,
                    row.version,
                    row.author,
                    row.description,
                    row.manifest_json,
                    row.enabled,
                    row.installed_at.to_rfc3339(),
                ],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| StoreError::Database(format!("spawn_blocking join: {e}")))?
    }

    pub(super) async fn list_connectors_impl(&self) -> Result<Vec<ConnectorRow>, StoreError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            let mut stmt =
                conn.prepare("SELECT name, version, author, description, manifest_json, enabled, installed_at FROM connectors ORDER BY name")?;
            let rows = stmt
                .query_map([], |row| {
                    Ok(ConnectorRow {
                        name: row.get(0)?,
                        version: row.get(1)?,
                        author: row.get(2)?,
                        description: row.get(3)?,
                        manifest_json: row.get(4)?,
                        enabled: row.get(5)?,
                        installed_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(6)?)
                            .map(|dt| dt.with_timezone(&Utc))
                            .map_err(|e| {
                                rusqlite::Error::FromSqlConversionFailure(
                                    6,
                                    rusqlite::types::Type::Text,
                                    Box::new(e),
                                )
                            })?,
                    })
                })?
                .collect::<Result<Vec<ConnectorRow>, _>>()?;
            Ok(rows)
        })
        .await
        .map_err(|e| StoreError::Database(format!("spawn_blocking join: {e}")))?
    }

    pub(super) async fn set_connector_enabled_impl(
        &self,
        name: &str,
        enabled: bool,
    ) -> Result<(), StoreError> {
        let conn = self.conn.clone();
        let name = name.to_owned();
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            let updated = conn.execute(
                "UPDATE connectors SET enabled = ?1 WHERE name = ?2",
                params![enabled, name],
            )?;
            if updated == 0 {
                return Err(StoreError::NotFound {
                    entity: "connector".into(),
                    id: name,
                });
            }
            Ok(())
        })
        .await
        .map_err(|e| StoreError::Database(format!("spawn_blocking join: {e}")))?
    }

    pub(super) async fn remove_connector_impl(&self, name: &str) -> Result<(), StoreError> {
        let conn = self.conn.clone();
        let name = name.to_owned();
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            let deleted = conn.execute("DELETE FROM connectors WHERE name = ?1", params![name])?;
            if deleted == 0 {
                return Err(StoreError::NotFound {
                    entity: "connector".into(),
                    id: name,
                });
            }
            Ok(())
        })
        .await
        .map_err(|e| StoreError::Database(format!("spawn_blocking join: {e}")))?
    }
}
