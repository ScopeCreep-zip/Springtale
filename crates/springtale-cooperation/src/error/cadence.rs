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
