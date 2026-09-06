//! Long-lived named API tokens (plan 6.6, finding 109).
//!
//! See `schema/sql/api_tokens.sql` for the DDL. The row never holds the
//! token itself — only `sha256(token)` — so the store can verify a
//! presented bearer without ever being able to produce one.

use serde::{Deserialize, Serialize};

/// One long-lived API token, as stored.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiTokenRow {
    /// UUID string. The handle `DELETE /auth/tokens/{id}` revokes.
    pub id: String,
    /// User-chosen label (`springtale-cli@laptop`, `dashboard`, …).
    pub name: String,
    /// `sha256(token bytes)` — 32 bytes. Never the token.
    pub token_hash: Vec<u8>,
    /// Unix ms at creation.
    pub created_at: i64,
    /// Unix ms of the last accepted request, `None` until first use.
    pub last_used: Option<i64>,
}
