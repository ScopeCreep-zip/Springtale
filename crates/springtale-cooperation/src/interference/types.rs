//! Interference types — conflict between cooperating agents.
//!
//! Per COOPERATION.md §13: four kinds of interference detected by comparing
//! ActionRecords pairwise. Per event sourcing: each tick is a transaction
//! with a read-set and write-set. Per LangGraph: reducer conflict = two
//! agents update the same state key.

use std::collections::{HashMap, HashSet};

use crate::cadence::AgentId;
use crate::tick::TickId;
use crate::types::WorkspaceKey;

/// A detected interference event between two agents.
pub struct InterferenceEvent {
    pub tick_sequence: TickId,
    pub agent_a: AgentId,
    pub agent_b: AgentId,
    pub interference_type: InterferenceType,
    pub severity: f32,
}

/// Types of interference between agents.
///
/// Per COOPERATION.md §13.1:
/// 1. ResourceConflict — both agents modified the same resource
/// 2. ActionNegation — A undid B's work
/// 3. CollateralDamage — A's side effects harmed B
/// 4. Redundancy — both did the same thing (wasted effort)
pub enum InterferenceType {
    /// Both agents wrote the same key with different values.
    /// Per LangGraph: same state field, no reducer → last-write-wins conflict.
    ResourceConflict,
    /// Agent A's write overwrote agent B's recent write (detected via read-after-write).
    ActionNegation,
    /// Agent A's side effect touched agent B's read surface.
    /// Per Divinity: fire on ground hurts anyone standing in it.
    CollateralDamage,
    /// Both agents wrote the same key with the same value.
    /// Per event sourcing: idempotent duplicate — wasted effort, not harmful.
    Redundancy,
}

/// What an agent actually did during a tick — the input to the detector.
///
/// Per COOPERATION.md §13.3 and event sourcing: each agent's tick actions
/// are recorded as a transaction with read-set, write-set (with values),
/// and side-effects (typed impacts on the environment).
///
/// Per OT (Operational Transform): conflict detection compares write-sets
/// pairwise. Same key + different value = conflict. Same key + same value
/// = redundancy.
#[derive(Debug, Clone, Default)]
pub struct ActionRecord {
    pub agent: AgentId,
    /// Keys the agent read this tick (blackboard reads, awareness queries).
    pub read_set: HashSet<WorkspaceKey>,
    /// Keys the agent wrote this tick, with the actual values written.
    /// The values matter: same key + same value = Redundancy (idempotent),
    /// same key + different value = ResourceConflict.
    pub write_set: HashMap<WorkspaceKey, serde_json::Value>,
    /// Side-effects that impact the environment beyond the write-set.
    /// Per Divinity: elemental surface damage. Per Helldivers: orbital strike blast radius.
    pub side_effects: Vec<SideEffect>,
}

impl ActionRecord {
    pub fn new(agent: AgentId) -> Self {
        Self {
            agent,
            ..Default::default()
        }
    }

    pub fn with_read(mut self, key: impl Into<WorkspaceKey>) -> Self {
        self.read_set.insert(key.into());
        self
    }

    pub fn with_write(mut self, key: impl Into<WorkspaceKey>, value: serde_json::Value) -> Self {
        self.write_set.insert(key.into(), value);
        self
    }

    pub fn with_side_effect(mut self, key: impl Into<WorkspaceKey>, magnitude: f32) -> Self {
        self.side_effects.push(SideEffect {
            affected_key: key.into(),
            magnitude,
        });
        self
    }
}

/// A side-effect of an agent's action that may affect other agents.
///
/// Per Divinity: fire on ground = SideEffect { affected_key: "ground:3,4", magnitude: 0.8 }.
/// Per Helldivers: orbital strike = SideEffect { affected_key: "zone:alpha", magnitude: 1.0 }.
#[derive(Debug, Clone)]
pub struct SideEffect {
    /// Which environment key or surface this effect touches.
    pub affected_key: WorkspaceKey,
    /// How severe (0.0 = negligible, 1.0 = destructive).
    pub magnitude: f32,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn action_record_builder() {
        let record = ActionRecord::new(AgentId::new())
            .with_read("config:api_key")
            .with_write("issues:42", serde_json::json!({"status": "closed"}))
            .with_side_effect("rate_limit:github", 0.3);

        assert_eq!(record.read_set.len(), 1);
        assert!(
            record
                .read_set
                .contains(&WorkspaceKey::from("config:api_key"))
        );
        assert_eq!(record.write_set.len(), 1);
        assert_eq!(
            record.write_set.get(&WorkspaceKey::from("issues:42")),
            Some(&serde_json::json!({"status": "closed"}))
        );
        assert_eq!(record.side_effects.len(), 1);
        assert_eq!(record.side_effects[0].affected_key, *"rate_limit:github");
        assert!((record.side_effects[0].magnitude - 0.3).abs() < f32::EPSILON);
    }

    #[test]
    fn interference_event_fields() {
        let a = AgentId::new();
        let b = AgentId::new();
        let event = InterferenceEvent {
            tick_sequence: TickId(42),
            agent_a: a,
            agent_b: b,
            interference_type: InterferenceType::ResourceConflict,
            severity: 0.8,
        };
        assert_eq!(event.tick_sequence, TickId(42));
        assert_eq!(event.agent_a, a);
        assert_eq!(event.agent_b, b);
        assert!((event.severity - 0.8).abs() < f32::EPSILON);
    }

    #[test]
    fn interference_types_distinct() {
        let types = [
            InterferenceType::ResourceConflict,
            InterferenceType::ActionNegation,
            InterferenceType::CollateralDamage,
            InterferenceType::Redundancy,
        ];
        assert_eq!(types.len(), 4);
    }
}
