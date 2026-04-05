use std::time::Duration;

/// Sentinel evaluation result for an action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Action may proceed.
    Go,
    /// Action should be delayed by the specified duration.
    Throttle(Duration),
    /// All pipelines paused. Reason provided.
    Pause(String),
    /// Connector quarantined. Reason provided.
    Quarantine(String),
}

impl Verdict {
    /// Whether this verdict allows the action to proceed (possibly after delay).
    pub fn is_allowed(&self) -> bool {
        matches!(self, Verdict::Go | Verdict::Throttle(_))
    }
}
