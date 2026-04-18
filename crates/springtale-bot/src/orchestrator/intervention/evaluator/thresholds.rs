/// Configurable cutoffs for the rule-based evaluator. All thresholds live
/// here so a future settings UI can tune L6 behaviour without editing
/// evaluator logic.
#[derive(Debug, Clone, Copy)]
pub struct InterventionThresholds {
    /// Cascade hits that trigger a `ChangeIntent` to Stabilize.
    pub cascade_stabilize: u32,
    /// Rally-token floor below which ForcedDissolve kicks in once failures compound.
    pub rally_dissolve_floor: u32,
    /// Cold duration (ticks) after which we escalate to the user.
    pub cold_escalate_ticks: u32,
    /// Fraction of members incapacitated that makes dissolve mandatory.
    /// Stored as (numerator, denominator) to keep the config `Copy`.
    pub incapacitated_ratio: (u32, u32),
}

impl Default for InterventionThresholds {
    fn default() -> Self {
        Self {
            cascade_stabilize: 2,
            rally_dissolve_floor: 0,
            cold_escalate_ticks: 600,
            incapacitated_ratio: (1, 2),
        }
    }
}

impl InterventionThresholds {
    pub fn is_terminal_incapacitation(&self, incapacitated: u32, total: u32) -> bool {
        let (num, den) = self.incapacitated_ratio;
        total > 0 && incapacitated * den >= total * num
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn half_incapacitated_is_terminal_at_default() {
        let t = InterventionThresholds::default();
        assert!(t.is_terminal_incapacitation(2, 4));
        assert!(!t.is_terminal_incapacitation(1, 4));
    }
}
