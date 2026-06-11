use rusqlite::{OptionalExtension, params};

use crate::error::StoreError;
use crate::schema::execution::ExecutionResultRow;
use crate::schema::wasm::WasmBinaryRow;

use super::SqliteBackend;

impl SqliteBackend {
    pub(super) async fn store_wasm_binary_impl(
        &self,
        name: &str,
        wasm_bytes: &[u8],
        manifest_json: &str,
        wasm_hash: &str,
        author: &str,
        author_pubkey_hex: &str,
        manifest_sig_hex: &str,
    ) -> Result<(), StoreError> {
        let conn = self.conn.clone();
        let name = name.to_owned();
        let wasm_bytes = wasm_bytes.to_owned();
        let manifest_json = manifest_json.to_owned();
        let wasm_hash = wasm_hash.to_owned();
        let author = author.to_owned();
        let author_pubkey_hex = author_pubkey_hex.to_owned();
        let manifest_sig_hex = manifest_sig_hex.to_owned();
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            conn.execute(
                "INSERT INTO wasm_binaries (name, wasm_bytes, manifest_json, wasm_hash, author, author_pubkey_hex, manifest_sig_hex, installed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, datetime('now'))
                 ON CONFLICT(name) DO UPDATE SET \
                   wasm_bytes        = excluded.wasm_bytes, \
                   manifest_json     = excluded.manifest_json, \
                   wasm_hash         = excluded.wasm_hash, \
                   author            = excluded.author, \
                   author_pubkey_hex = excluded.author_pubkey_hex, \
                   manifest_sig_hex  = excluded.manifest_sig_hex, \
                   installed_at      = excluded.installed_at",
                rusqlite::params![name, wasm_bytes, manifest_json, wasm_hash, author, author_pubkey_hex, manifest_sig_hex],
            )
            .map_err(|e| StoreError::Database(format!("store_wasm_binary: {e}")))?;
            Ok(())
        })
        .await
        .map_err(|e| StoreError::Database(format!("spawn_blocking join: {e}")))?
    }

    pub(super) async fn get_wasm_binary_impl(
        &self,
        name: &str,
    ) -> Result<Option<WasmBinaryRow>, StoreError> {
        let conn = self.conn.clone();
        let name = name.to_owned();
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            let mut stmt = conn
                .prepare("SELECT name, wasm_bytes, manifest_json, wasm_hash, author, author_pubkey_hex, manifest_sig_hex, installed_at FROM wasm_binaries WHERE name = ?1")
                .map_err(|e| StoreError::Database(format!("get_wasm_binary prepare: {e}")))?;
            let row = stmt
                .query_row(rusqlite::params![name], |row| {
                    Ok(WasmBinaryRow {
                        name: row.get(0)?,
                        wasm_bytes: row.get(1)?,
                        manifest_json: row.get(2)?,
                        wasm_hash: row.get(3)?,
                        author: row.get(4)?,
                        author_pubkey_hex: row.get(5)?,
                        manifest_sig_hex: row.get(6)?,
                        installed_at: chrono::DateTime::parse_from_rfc3339(
                            &row.get::<_, String>(7)?,
                        )
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .unwrap_or_else(|_| chrono::Utc::now()),
                    })
                })
                .optional()
                .map_err(|e| StoreError::Database(format!("get_wasm_binary query: {e}")))?;
            Ok(row)
        })
        .await
        .map_err(|e| StoreError::Database(format!("spawn_blocking join: {e}")))?
    }

    pub(super) async fn list_wasm_binaries_impl(&self) -> Result<Vec<WasmBinaryRow>, StoreError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            let mut stmt = conn
                .prepare("SELECT name, wasm_bytes, manifest_json, wasm_hash, author, author_pubkey_hex, manifest_sig_hex, installed_at FROM wasm_binaries ORDER BY name")
                .map_err(|e| StoreError::Database(format!("list_wasm_binaries prepare: {e}")))?;
            let rows = stmt
                .query_map([], |row| {
                    Ok(WasmBinaryRow {
                        name: row.get(0)?,
                        wasm_bytes: row.get(1)?,
                        manifest_json: row.get(2)?,
                        wasm_hash: row.get(3)?,
                        author: row.get(4)?,
                        author_pubkey_hex: row.get(5)?,
                        manifest_sig_hex: row.get(6)?,
                        installed_at: chrono::DateTime::parse_from_rfc3339(
                            &row.get::<_, String>(7)?,
                        )
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .unwrap_or_else(|_| chrono::Utc::now()),
                    })
                })
                .map_err(|e| StoreError::Database(format!("list_wasm_binaries query: {e}")))?;
            let mut entries = Vec::new();
            for row in rows {
                entries.push(
                    row.map_err(|e| {
                        StoreError::Database(format!("list_wasm_binaries row: {e}"))
                    })?,
                );
            }
            Ok(entries)
        })
        .await
        .map_err(|e| StoreError::Database(format!("spawn_blocking join: {e}")))?
    }

    pub(super) async fn delete_wasm_binary_impl(&self, name: &str) -> Result<(), StoreError> {
        let conn = self.conn.clone();
        let name = name.to_owned();
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            conn.execute(
                "DELETE FROM wasm_binaries WHERE name = ?1",
                rusqlite::params![name],
            )
            .map_err(|e| StoreError::Database(format!("delete_wasm_binary: {e}")))?;
            Ok(())
        })
        .await
        .map_err(|e| StoreError::Database(format!("spawn_blocking join: {e}")))?
    }

    pub(super) async fn insert_execution_result_impl(
        &self,
        input: &crate::schema::execution::ExecutionResultInput<'_>,
    ) -> Result<(), StoreError> {
        let conn = self.conn.clone();
        let id = input.id.to_owned();
        let connector_name = input.connector_name.to_owned();
        let rule_id = input.rule_id.map(|s| s.to_owned());
        let rule_name = input.rule_name.map(|s| s.to_owned());
        let output_json = input.output_json.to_owned();
        let success = input.success;
        let error_message = input.error_message.map(|s| s.to_owned());
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            conn.execute(
                "INSERT INTO execution_results (id, connector_name, rule_id, rule_name, output_json, success, error_message, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, datetime('now'))",
                params![id, connector_name, rule_id, rule_name, output_json, success as i32, error_message],
            )?;
            // Cap at 100 results per connector
            conn.execute(
                "DELETE FROM execution_results WHERE id IN (
                    SELECT id FROM execution_results WHERE connector_name = ?1
                    ORDER BY created_at DESC LIMIT -1 OFFSET 100
                )",
                params![connector_name],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| StoreError::Database(format!("spawn_blocking join: {e}")))?
    }

    pub(super) async fn list_execution_results_impl(
        &self,
        connector_name: &str,
        limit: usize,
    ) -> Result<Vec<ExecutionResultRow>, StoreError> {
        let conn = self.conn.clone();
        let connector_name = connector_name.to_owned();
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            let mut stmt = conn.prepare(
                "SELECT id, connector_name, rule_name, output_json, success, error_message, created_at
                 FROM execution_results WHERE connector_name = ?1
                 ORDER BY created_at DESC LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![connector_name, limit as i64], |row| {
                Ok(ExecutionResultRow {
                    id: row.get(0)?,
                    connector_name: row.get(1)?,
                    rule_name: row.get(2)?,
                    output_json: row.get(3)?,
                    success: row.get::<_, i32>(4)? != 0,
                    error_message: row.get(5)?,
                    created_at: row.get(6)?,
                })
            })?;
            let mut results = Vec::new();
            for row in rows {
                results.push(row?);
            }
            Ok(results)
        })
        .await
        .map_err(|e| StoreError::Database(format!("spawn_blocking join: {e}")))?
    }
}
