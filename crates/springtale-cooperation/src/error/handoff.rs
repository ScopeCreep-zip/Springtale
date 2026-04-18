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
}
