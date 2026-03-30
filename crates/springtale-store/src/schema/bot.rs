use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Row type for the `bot_sessions` table.
///
/// Tracks per-(user, channel) conversation state for multi-step command flows.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRow {
    /// User identifier (platform-specific, e.g., Telegram user ID).
    pub user_id: String,
    /// Channel identifier (platform-specific, e.g., Telegram chat ID).
    pub channel_id: String,
    /// What the bot last said (for context in multi-step flows).
    pub last_bot_message: Option<String>,
    /// Command the bot is waiting for input on (multi-step state).
    pub pending_command: Option<String>,
    /// Arbitrary handler state as JSON string.
    pub state_data: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Row type for the `user_prefs` table.
///
/// Per-user preferences persisted in SQLite.
/// Notifications default to off (IPV safety requirement — §2.8).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPrefsRow {
    pub user_id: String,
    /// IANA timezone (e.g., "America/New_York"). Default: "UTC".
    pub timezone: String,
    /// Language code (e.g., "en"). Default: "en".
    pub language: String,
    /// Whether OS/push notifications are enabled. Default: false.
    pub notifications_enabled: bool,
    pub updated_at: DateTime<Utc>,
}

/// Row type for the `bot_memory` table.
///
/// Stores encrypted conversation memory entries with provenance tracking.
/// Content is encrypted via vault (XChaCha20-Poly1305) before storage.
#[derive(Debug, Clone)]
pub struct MemoryRow {
    /// Unique entry identifier.
    pub id: String,
    pub user_id: String,
    pub channel_id: String,
    /// Memory category: "conversation", "fact", "preference".
    pub category: String,
    /// Schema version for forward compatibility.
    pub schema_version: i64,
    /// Who wrote this entry: "user", "agent", "connector".
    pub author: String,
    /// How this entry was created: "user_input", "connector_output",
    /// "ai_generated", "compacted".
    pub source: String,
    /// Ciphertext (XChaCha20-Poly1305 encrypted content).
    pub content_encrypted: Vec<u8>,
    /// 24-byte nonce for XChaCha20-Poly1305.
    pub nonce: Vec<u8>,
    /// SHA-256 hash of plaintext for dedup without decrypting.
    pub content_hash: Option<String>,
    /// Previous version ID (for compaction chain tracking).
    pub parent_id: Option<String>,
    /// Trust score: 1.0 for user input, lower for AI-generated.
    pub trust_score: f64,
    pub created_at: DateTime<Utc>,
    /// Optional TTL — entry may be cleaned up after this time.
    pub expires_at: Option<DateTime<Utc>>,
}
