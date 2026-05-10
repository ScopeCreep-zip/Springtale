//! Cooperation row types — §13 atomic CAS + §20 environment-mediated handoff.

/// Outcome of an atomic compare-and-swap write. Mirrors the semantics of
/// `sled::Tree::compare_and_swap` — on mismatch the caller sees the current
/// value and the writer who last set it, so interference classification
/// (ResourceConflict vs Redundancy) is possible without a second query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoopCasOutcome {
    /// CAS succeeded — the proposed value is now in the store.
    Applied,
    /// CAS failed — another writer's value is in the store.
    /// `current_value == proposed` means redundant overlap (both tried to
    /// write the same result); otherwise it's a real resource conflict.
    Mismatch {
        current_value: Vec<u8>,
        current_writer: String,
        current_tick: i64,
    },
}

/// Single row of the coop_writes table.
#[derive(Debug, Clone)]
pub struct CoopWriteRow {
    pub key: String,
    pub value: Vec<u8>,
    pub writer: String,
    pub tick: i64,
}

/// Single row of the coop_deposits table.
#[derive(Debug, Clone)]
pub struct CoopDepositRow {
    pub location: String,
    pub payload: Vec<u8>,
    pub depositor: String,
    pub deposited_at: i64,
    pub expires_at: Option<i64>,
    pub claimed_by: Option<String>,
}
