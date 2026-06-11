//! Bundle conversion — bridges `SharedMentalModel` (cooperation domain
//! types) to `MentalModelBundle` (store-layer row types) and back.
//!
//! The in-memory types carry `Instant` (process-local, not portable). We
//! convert to Unix epoch seconds at save time and reconstitute with
//! `Instant::now()` on load — relative ordering is lost across process
//! boundaries, but the *content* is preserved. Callers using `learned_at`
//! for reasoning already know it's approximate after a restart.

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use springtale_store::{
    MentalModelBundle, MentalModelCapabilityRow, MentalModelConventionRow, MentalModelDomainRow,
    MentalModelPatternRow, MentalModelVocabularyRow,
};

use crate::cadence::AgentId;
use crate::mental_model::types::{
    Convention, CooperationPattern, DomainEntry, SharedMentalModel, VocabularyEntry,
};

use super::error::StoreError;

/// Current wall-clock time as Unix epoch seconds. Returns 0 if the system
/// clock is before UNIX_EPOCH (clocks that broken will fail elsewhere first).
pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Convert a SharedMentalModel into a MentalModelBundle for persistence.
pub fn to_bundle(model: &SharedMentalModel) -> Result<MentalModelBundle, StoreError> {
    let domain = model
        .domain_knowledge
        .iter()
        .map(|(key, entry)| MentalModelDomainRow {
            key: key.clone(),
            description: entry.description.clone(),
            learned_at_unix: now_unix(),
            confidence: entry.confidence,
        })
        .collect();

    let capability = model
        .capability_awareness
        .iter()
        .flat_map(|(agent, caps)| {
            let agent_id = agent.0.to_string();
            caps.iter().map(move |cap| MentalModelCapabilityRow {
                agent_id: agent_id.clone(),
                capability: cap.name.clone(),
            })
        })
        .collect();

    let pattern: Result<Vec<_>, StoreError> = model
        .cooperation_patterns
        .iter()
        .map(|p| {
            let participants: Vec<String> =
                p.participants.iter().map(|a| a.0.to_string()).collect();
            Ok(MentalModelPatternRow {
                trigger_text: p.trigger.0.clone(),
                participants_json: serde_json::to_string(&participants)
                    .map_err(|e| StoreError::Serialization(e.to_string()))?,
                success_count: p.success_count,
                failure_count: p.failure_count,
                last_used_unix: now_unix(),
            })
        })
        .collect();

    let vocabulary: Result<Vec<_>, StoreError> = model
        .shared_vocabulary
        .values()
        .map(|entry| {
            let ids: Vec<String> = entry
                .established_by
                .iter()
                .map(|a| a.0.to_string())
                .collect();
            Ok(MentalModelVocabularyRow {
                term: entry.term.clone(),
                meaning: entry.meaning.clone(),
                established_by_json: serde_json::to_string(&ids)
                    .map_err(|e| StoreError::Serialization(e.to_string()))?,
            })
        })
        .collect();

    let convention: Result<Vec<_>, StoreError> = model
        .conventions
        .iter()
        .map(|c| {
            let ids: Vec<String> = c.established_by.iter().map(|a| a.0.to_string()).collect();
            Ok(MentalModelConventionRow {
                description: c.description.clone(),
                established_by_json: serde_json::to_string(&ids)
                    .map_err(|e| StoreError::Serialization(e.to_string()))?,
                strength: c.strength,
            })
        })
        .collect();

    Ok(MentalModelBundle {
        domain,
        capability,
        pattern: pattern?,
        vocabulary: vocabulary?,
        convention: convention?,
    })
}

/// Reconstitute a SharedMentalModel from a loaded bundle.
pub fn from_bundle(bundle: MentalModelBundle) -> Result<SharedMentalModel, StoreError> {
    let mut model = SharedMentalModel::default();

    for r in bundle.domain {
        let key = r.key.clone();
        model.domain_knowledge.insert(
            key,
            DomainEntry {
                description: r.description,
                learned_at: Instant::now(),
                confidence: r.confidence,
            },
        );
    }

    for r in bundle.capability {
        let id = parse_agent_id(&r.agent_id)?;
        model
            .capability_awareness
            .entry(id)
            .or_default()
            .push(crate::capability::CapabilityDecl::new(r.capability));
    }

    for r in bundle.pattern {
        let ids: Vec<String> = serde_json::from_str(&r.participants_json)
            .map_err(|e| StoreError::Serialization(e.to_string()))?;
        let participants = ids
            .iter()
            .map(|s| parse_agent_id(s))
            .collect::<Result<Vec<_>, _>>()?;
        model.cooperation_patterns.push(CooperationPattern {
            trigger: r.trigger_text.into(),
            participants,
            success_count: r.success_count,
            failure_count: r.failure_count,
            last_used: Instant::now(),
        });
    }

    for r in bundle.vocabulary {
        let ids: Vec<String> = serde_json::from_str(&r.established_by_json)
            .map_err(|e| StoreError::Serialization(e.to_string()))?;
        let established_by = ids
            .iter()
            .map(|s| parse_agent_id(s))
            .collect::<Result<Vec<_>, _>>()?;
        let term = r.term.clone();
        model.shared_vocabulary.insert(
            term,
            VocabularyEntry {
                term: r.term,
                meaning: r.meaning,
                established_by,
            },
        );
    }

    for r in bundle.convention {
        let ids: Vec<String> = serde_json::from_str(&r.established_by_json)
            .map_err(|e| StoreError::Serialization(e.to_string()))?;
        let established_by = ids
            .iter()
            .map(|s| parse_agent_id(s))
            .collect::<Result<Vec<_>, _>>()?;
        model.conventions.push(Convention {
            description: r.description,
            established_by,
            strength: r.strength,
        });
    }

    Ok(model)
}

pub(crate) fn parse_agent_id(s: &str) -> Result<AgentId, StoreError> {
    uuid::Uuid::parse_str(s)
        .map(AgentId)
        .map_err(|e| StoreError::InvalidRow(format!("invalid agent_id {s}: {e}")))
}
