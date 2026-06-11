//! Row type for the `ai_token_usage` table — backs the
//! `SqliteTokenQuota` per-bot daily quota (OWASP LLM10).
//!
//! `day_ymd` is the year×1000 + ordinal-day-of-year packing used by
//! the in-process quota — keeps the two backends interchangeable so
//! the runtime can swap them at boot.

use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct AiTokenUsageRow {
    pub agent_id: String,
    pub day_ymd: u32,
    pub tokens_used: u64,
}
