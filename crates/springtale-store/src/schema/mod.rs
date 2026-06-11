pub mod ai_token_usage;
pub mod apply;
pub mod audit;
pub mod audit_chain;
pub mod bot;
pub mod connectors;
pub mod events;
pub mod jobs;
pub mod rules;
pub mod safety;

#[cfg(test)]
mod tests;

pub use apply::{SCHEMA_VERSION, apply as apply_schema, is_legacy_database};

pub use ai_token_usage::AiTokenUsageRow;
pub use audit::{AuditEntry, AuditFilter};
pub use audit_chain::{compute_row_hash, vault_genesis_anchor};
pub use bot::{MemoryRow, SessionRow, UserPrefsRow};
pub use connectors::ConnectorRow;
pub use events::{EventEntry, EventFilter};
pub use jobs::{JobId, JobRow};
pub use rules::RuleRow;
pub use safety::SafetyConfigRow;
pub mod execution;
pub mod formations;
pub use execution::ExecutionResultRow;
pub use formations::{FormationMemberRow, FormationMomentumRow, FormationRallyRow, FormationRow};
pub mod wasm;
pub use wasm::WasmBinaryRow;
pub mod cooperation;
pub use cooperation::{CoopCasOutcome, CoopDepositRow, CoopWriteRow};
pub mod mental_model;
pub use mental_model::{
    MentalModelBundle, MentalModelCapabilityRow, MentalModelConventionRow, MentalModelDomainRow,
    MentalModelPatternRow, MentalModelVocabularyRow, MentalModelWorkspaceRow,
};
pub mod approvals;
pub use approvals::{PendingApprovalRow, ToolLoopCheckpointRow};
pub mod dedupe;
pub use dedupe::DedupeOutcome;
pub mod executions;
pub use executions::{
    ExecutionFilter, ExecutionMode, ExecutionRow, ExecutionStatus, ExecutionStepRow,
    ExecutionSummary, MomentumTag, StepStatus,
};
