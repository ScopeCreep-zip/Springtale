//! Interference detection — conflict between cooperating agents.
//!
//! Per COOPERATION.pdf §13:
//! Game sources: Helldivers 2 friendly fire, Divinity combos hitting
//! allies, Total War archers hitting own infantry.
//!
//! "The most common source of party wipes in co-op isn't enemy damage
//! — it's friendly combo chains gone wrong." (Divinity OS2)
//!
//! Interference decreases momentum (§7). Repeated interference
//! triggers role adaptation (§14).

use super::cadence::AgentId;

/// A detected interference event between two agents.
///
/// From COOPERATION.pdf §13:
/// ```text
/// pub struct InterferenceEvent {
///     pub tick_sequence: u64,
///     pub agent_a: AgentId,
///     pub agent_b: AgentId,
///     pub interference_type: InterferenceType,
///     pub severity: f32,
/// }
/// ```
pub struct InterferenceEvent {
    pub tick_sequence: u64,
    pub agent_a: AgentId,
    pub agent_b: AgentId,
    pub interference_type: InterferenceType,
    pub severity: f32,
}

/// Types of interference between agents.
///
/// From COOPERATION.pdf §13:
/// ```text
/// pub enum InterferenceType {
///     ResourceConflict,   // both modified same resource
///     ActionNegation,     // A undid B's work
///     CollateralDamage,   // A's side effects harmed B
///     Redundancy,         // both did the same thing
/// }
/// ```
pub enum InterferenceType {
    /// Both agents modified the same resource. Environment write conflict.
    ResourceConflict,
    /// Agent A's action undid agent B's work.
    ActionNegation,
    /// Agent A's side effects harmed agent B. Helldivers orbital strike.
    CollateralDamage,
    /// Both agents did the same thing. Wasted effort.
    Redundancy,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_interference_event() {
        let event = InterferenceEvent {
            tick_sequence: 42,
            agent_a: AgentId::new(),
            agent_b: AgentId::new(),
            interference_type: InterferenceType::ResourceConflict,
            severity: 0.7,
        };
        assert_eq!(event.tick_sequence, 42);
        assert!(event.severity > 0.5);
    }

    #[test]
    fn test_all_interference_types() {
        let _rc = InterferenceType::ResourceConflict;
        let _an = InterferenceType::ActionNegation;
        let _cd = InterferenceType::CollateralDamage;
        let _rd = InterferenceType::Redundancy;
    }
}
