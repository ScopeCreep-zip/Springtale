//! Dedupe table types — outcome returned to the dispatcher when a
//! chain's [`springtale_core::rule::action::Action::Dedupe`] step
//! runs. The dispatcher uses [`DedupeOutcome::SeenBefore`] to
//! short-circuit the chain (returns `ChainError::Suppressed`).

/// Result of `dedupe_check` — whether this key has been seen for the
/// `(formation_id, rule_id, bucket)` scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DedupeOutcome {
    /// Key was not in the table. The check-and-record was atomic; the
    /// key is now recorded so the next call with the same key returns
    /// `SeenBefore`.
    Fresh,

    /// Key was already in the table — the upstream content has been
    /// processed before, so the dispatcher should suppress downstream
    /// chain steps.
    SeenBefore,
}
