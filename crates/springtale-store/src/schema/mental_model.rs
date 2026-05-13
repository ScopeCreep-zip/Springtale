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

/// One row in `mental_model_workspaces` — an external destination
/// (Telegram chat / Discord channel / Signal group / IRC channel /
/// Nostr pubkey / Bluesky account) the formation has discovered.
///
/// Cross-crate boundary: the domain types
/// (`ExternalWorkspaceEntry`, `WorkspaceProvenance`,
/// `DiscoveredWorkspace`) live in `springtale-cooperation`. This
/// row is the persistence shape — flat scalars + JSON-serialized
/// blobs for the variant-shaped fields. Row ↔ domain conversion
/// lives in the cooperation crate (or the runtime workspaces
/// operations) so this crate stays domain-agnostic, matching the
/// existing `MentalModel*Row` pattern above.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MentalModelWorkspaceRow {
    /// URI form — `"telegram://chat/12345"`,
    /// `"discord://guild/G/channel/C"`, etc.
    pub workspace_key: String,
    pub connector_name: String,
    pub display_name: String,
    pub kind: String,
    /// `serde_json::Value` serialized to a string; `None` for
    /// empty `{}` to keep nullable column semantics.
    pub metadata_json: Option<String>,
    pub first_seen_at_unix_ms: i64,
    pub last_seen_at_unix_ms: i64,
    /// Serialized `WorkspaceProvenance` enum.
    pub provenance_json: String,
}
