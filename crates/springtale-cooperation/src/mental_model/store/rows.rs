//! Serializable row mirrors of the in-memory `SharedMentalModel` types.
//!
//! The in-memory types carry `Instant` (process-local, not portable). For
//! persistence we convert to Unix epoch seconds at save time and reconstitute
//! to `Instant::now()` on load (so "learned_at" shows as "now" post-reload;
//! the relative ordering is lost, but the *content* is preserved). That's
//! acceptable: callers using learned_at for reasoning already know it's
//! approximate after a cross-process boundary.

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::cadence::AgentId;
use crate::mental_model::types::{Convention, CooperationPattern, DomainEntry, VocabularyEntry};

use super::error::StoreError;

pub struct DomainRow {
    pub key: String,
    pub description: String,
    pub learned_at_unix: i64,
    pub confidence: f32,
}

pub struct CapabilityRow {
    pub agent_id: String,
    pub capability: String,
}

pub struct PatternRow {
    pub trigger_text: String,
    pub participants_json: String,
    pub success_count: u32,
    pub failure_count: u32,
    pub last_used_unix: i64,
}

pub struct VocabularyRow {
    pub term: String,
    pub meaning: String,
    pub established_by_json: String,
}

pub struct ConventionRow {
    pub description: String,
    pub established_by_json: String,
    pub strength: f32,
}

/// Current wall-clock time as Unix epoch seconds. Fallible — returns 0 if
/// the system clock is before UNIX_EPOCH (shouldn't happen in practice but
/// better than panicking).
pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

impl DomainRow {
    pub fn from_entry(key: &str, entry: &DomainEntry) -> Self {
        Self {
            key: key.to_owned(),
            description: entry.description.clone(),
            learned_at_unix: now_unix(),
            confidence: entry.confidence,
        }
    }

    pub fn into_entry(self) -> DomainEntry {
        DomainEntry {
            description: self.description,
            learned_at: Instant::now(),
            confidence: self.confidence,
        }
    }
}

impl PatternRow {
    pub fn from_pattern(p: &CooperationPattern) -> Result<Self, StoreError> {
        let participants: Vec<String> = p.participants.iter().map(|a| a.0.to_string()).collect();
        Ok(Self {
            trigger_text: p.trigger.0.clone(),
            participants_json: serde_json::to_string(&participants)
                .map_err(|e| StoreError::Serialization(e.to_string()))?,
            success_count: p.success_count,
            failure_count: p.failure_count,
            last_used_unix: now_unix(),
        })
    }

    pub fn into_pattern(self) -> Result<CooperationPattern, StoreError> {
        let ids: Vec<String> = serde_json::from_str(&self.participants_json)
            .map_err(|e| StoreError::Serialization(e.to_string()))?;
        let participants = ids
            .iter()
            .map(|s| parse_agent_id(s))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(CooperationPattern {
            trigger: self.trigger_text.into(),
            participants,
            success_count: self.success_count,
            failure_count: self.failure_count,
            last_used: Instant::now(),
        })
    }
}

impl VocabularyRow {
    pub fn from_entry(entry: &VocabularyEntry) -> Result<Self, StoreError> {
        let ids: Vec<String> = entry
            .established_by
            .iter()
            .map(|a| a.0.to_string())
            .collect();
        Ok(Self {
            term: entry.term.clone(),
            meaning: entry.meaning.clone(),
            established_by_json: serde_json::to_string(&ids)
                .map_err(|e| StoreError::Serialization(e.to_string()))?,
        })
    }

    pub fn into_entry(self) -> Result<VocabularyEntry, StoreError> {
        let ids: Vec<String> = serde_json::from_str(&self.established_by_json)
            .map_err(|e| StoreError::Serialization(e.to_string()))?;
        let established_by = ids
            .iter()
            .map(|s| parse_agent_id(s))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(VocabularyEntry {
            term: self.term,
            meaning: self.meaning,
            established_by,
        })
    }
}

impl ConventionRow {
    pub fn from_convention(c: &Convention) -> Result<Self, StoreError> {
        let ids: Vec<String> = c.established_by.iter().map(|a| a.0.to_string()).collect();
        Ok(Self {
            description: c.description.clone(),
            established_by_json: serde_json::to_string(&ids)
                .map_err(|e| StoreError::Serialization(e.to_string()))?,
            strength: c.strength,
        })
    }

    pub fn into_convention(self) -> Result<Convention, StoreError> {
        let ids: Vec<String> = serde_json::from_str(&self.established_by_json)
            .map_err(|e| StoreError::Serialization(e.to_string()))?;
        let established_by = ids
            .iter()
            .map(|s| parse_agent_id(s))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Convention {
            description: self.description,
            established_by,
            strength: self.strength,
        })
    }
}

pub(crate) fn parse_agent_id(s: &str) -> Result<AgentId, StoreError> {
    uuid::Uuid::parse_str(s)
        .map(AgentId)
        .map_err(|e| StoreError::InvalidRow(format!("invalid agent_id {s}: {e}")))
}
