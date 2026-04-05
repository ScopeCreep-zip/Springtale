/// Compaction strategy for conversation memory.
///
/// Phase 1b: simple truncation (drop oldest beyond max).
/// Phase 2a: AI summarization when adapter is available, with
/// truncation fallback if AI is unavailable.
pub enum CompactionStrategy {
    /// Drop entries beyond `max_entries`, keeping the newest.
    Truncate { max_entries: usize },
    /// Summarize oldest entries via AI, then truncate.
    /// Falls back to `Truncate` if AI is unavailable.
    AiSummarize { max_entries: usize },
}

impl Default for CompactionStrategy {
    fn default() -> Self {
        Self::Truncate { max_entries: 50 }
    }
}

impl CompactionStrategy {
    /// Get the max_entries limit regardless of strategy.
    pub fn max_entries(&self) -> usize {
        match self {
            Self::Truncate { max_entries } | Self::AiSummarize { max_entries } => *max_entries,
        }
    }
}
