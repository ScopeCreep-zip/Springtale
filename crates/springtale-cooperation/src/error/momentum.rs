use thiserror::Error;

use crate::momentum::MomentumTier;

#[derive(Debug, Error)]
pub enum MomentumError {
    #[error("COOP-3001: momentum insufficient: need {required:?}, have {current:?}")]
    Insufficient {
        required: MomentumTier,
        current: MomentumTier,
    },
    #[error("COOP-3002: capability locked at tier {0:?}")]
    CapabilityLocked(MomentumTier),
}

impl MomentumError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Insufficient { .. } => "COOP-3001",
            Self::CapabilityLocked(_) => "COOP-3002",
        }
    }
}
