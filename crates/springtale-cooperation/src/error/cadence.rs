use thiserror::Error;

#[derive(Debug, Error)]
pub enum CadenceError {
    #[error("COOP-1001: tick bus channel closed")]
    ChannelClosed,
    #[error("COOP-1002: tick sequence wrapped")]
    SequenceWrap,
    #[error("COOP-1003: subscriber lagged by {lagged} ticks")]
    Lagged { lagged: u64 },
}

impl CadenceError {
    /// Stable error ID — same string the Display impl prefixes. Used by
    /// `springtale fix <id>` to look up the remediation guide.
    pub const fn code(&self) -> &'static str {
        match self {
            Self::ChannelClosed => "COOP-1001",
            Self::SequenceWrap => "COOP-1002",
            Self::Lagged { .. } => "COOP-1003",
        }
    }
}
