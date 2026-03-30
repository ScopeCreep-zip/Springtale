/// Compaction strategy for conversation memory.
///
/// Phase 1b: simple truncation (drop oldest beyond max).
/// Phase 2a: AI summarization when adapter is available.
pub enum CompactionStrategy {
    /// Drop entries beyond `max_entries`, keeping the newest.
    Truncate { max_entries: usize },
}

impl Default for CompactionStrategy {
    fn default() -> Self {
        Self::Truncate { max_entries: 50 }
    }
}
