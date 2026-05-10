//! Mental-model row types — §21 shared context accumulation.
//!
//! Five tables, one per `SharedMentalModel` field in the cooperation crate.
//! Row conversion (domain-types ↔ rows) lives in the cooperation crate so
//! this crate stays domain-agnostic.

#[derive(Debug, Clone)]
pub struct MentalModelDomainRow {
    pub key: String,
    pub description: String,
    pub learned_at_unix: i64,
    pub confidence: f32,
}

#[derive(Debug, Clone)]
pub struct MentalModelCapabilityRow {
    pub agent_id: String,
    pub capability: String,
}

#[derive(Debug, Clone)]
pub struct MentalModelPatternRow {
    pub trigger_text: String,
    pub participants_json: String,
    pub success_count: u32,
    pub failure_count: u32,
    pub last_used_unix: i64,
}

#[derive(Debug, Clone)]
pub struct MentalModelVocabularyRow {
    pub term: String,
    pub meaning: String,
    pub established_by_json: String,
}

#[derive(Debug, Clone)]
pub struct MentalModelConventionRow {
    pub description: String,
    pub established_by_json: String,
    pub strength: f32,
}

/// Full snapshot of one formation's mental-model state. Used as the
/// transactional unit for save/load — either all five tables update
/// together or the operation fails.
#[derive(Debug, Clone, Default)]
pub struct MentalModelBundle {
    pub domain: Vec<MentalModelDomainRow>,
    pub capability: Vec<MentalModelCapabilityRow>,
    pub pattern: Vec<MentalModelPatternRow>,
    pub vocabulary: Vec<MentalModelVocabularyRow>,
    pub convention: Vec<MentalModelConventionRow>,
}
