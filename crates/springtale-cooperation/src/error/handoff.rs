use thiserror::Error;

use crate::capability::CapabilityDecl;

#[derive(Debug, Error)]
pub enum HandoffError {
    #[error("COOP-A001: no capable receiver for {required}")]
    NoCapableReceiver { required: CapabilityDecl },
    #[error("COOP-A002: handoff payload expired")]
    PayloadExpired,
    #[error("COOP-A003: return obligation unmet")]
    UnmetObligation,
    #[error("COOP-A004: serialize deposit payload: {0}")]
    SerializeDeposit(String),
    #[error("COOP-A005: deserialize deposit payload: {0}")]
    DeserializeDeposit(String),
}

impl HandoffError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::NoCapableReceiver { .. } => "COOP-A001",
            Self::PayloadExpired => "COOP-A002",
            Self::UnmetObligation => "COOP-A003",
            Self::SerializeDeposit(_) => "COOP-A004",
            Self::DeserializeDeposit(_) => "COOP-A005",
        }
    }
}
