use thiserror::Error;

/// Top-level error type for springtale-store.
#[derive(Debug, Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Database(String),

    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("migration failed: {0}")]
    Migration(String),

    #[error("record not found: {entity} with id {id}")]
    NotFound { entity: String, id: String },

    #[error("serialization error: {0}")]
    Serialization(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("database file has insecure permissions")]
    InsecurePermissions,
}
