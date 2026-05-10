//! Reaction table — data-driven surface interaction rules.
//!
//! Per Noita: `data/materials.xml` is a flat table of `<Reaction>` entries.
//! Per CDDA: `field_type.json` defines field-to-field transformations.
//! Per DOS2: surface combos are engine-checked at stamp time.
//!
//! Our table is a `Vec<SurfaceReaction>` scanned linearly — cooperation
//! formations have O(10) canonical surface types, not O(10K) like Noita's
//! full material sim.

use super::reaction::{ReactionOutput, SurfaceReaction};

/// Canonical cooperation surface vocabulary. Every tag on both sides of a
/// reaction is produced by a real subsystem elsewhere in this crate and
/// consumed by another real subsystem — no decorative entries.
///
/// | Tag              | Produced by                    | Consumed by        |
/// |------------------|--------------------------------|--------------------|
/// | `fresh_input`    | external event arrival         | §10 / §9           |
/// | `high_attention` | `attention::AttentionEconomy`  | §10 composition    |
/// | `urgent_response`| composition result             | routing/dispatch   |
/// | `rally_beacon`   | `rally::RallyTokens` consumption | recovery agents  |
/// | `fatigue`        | `pacing::PacingState` phase    | damper of signals  |
/// | `cooldown`       | post-interference dampener     | routing suppressor |
/// | `handoff_ready`  | handoff producer completion    | §20 FlexibleChain  |
/// | `role_vacancy`   | transformation trigger         | §14 recomposition  |
/// | `consensus_call` | consensus proposal broadcast   | §11 voter agents   |
pub mod tags {
    pub const FRESH_INPUT: &str = "fresh_input";
    pub const HIGH_ATTENTION: &str = "high_attention";
    pub const URGENT_RESPONSE: &str = "urgent_response";
    pub const RALLY_BEACON: &str = "rally_beacon";
    pub const FATIGUE: &str = "fatigue";
    pub const COOLDOWN: &str = "cooldown";
    pub const HANDOFF_READY: &str = "handoff_ready";
    pub const ROLE_VACANCY: &str = "role_vacancy";
    pub const CONSENSUS_CALL: &str = "consensus_call";
}

/// Lookup table for surface reactions.
///
/// Per Noita's materials.xml: a flat list scanned for matching pairs.
/// First match wins. Probability < 1.0 means stochastic (checked by caller).
#[derive(Debug, Clone)]
pub struct ReactionTable {
    reactions: Vec<SurfaceReaction>,
}

impl ReactionTable {
    pub fn new() -> Self {
        Self {
            reactions: Vec::new(),
        }
    }

    /// Canonical Springtale reaction table. Every reaction's inputs and
    /// outputs belong to the 9-tag cooperation vocabulary. Game-literal
    /// tags (water, fire, oil, lava, electricity) are NOT present —
    /// Springtale's surfaces model coordination signals, not physics.
    pub fn cooperation_defaults() -> Self {
        use tags::*;
        let mut t = Self::new();

        // fresh_input + high_attention → urgent_response.
        // (Input becomes actionable when attention is locked onto it;
        //  both inputs are consumed because attention and input are now
        //  jointly represented by the urgent_response surface.)
        t.add(SurfaceReaction::new(
            FRESH_INPUT,
            HIGH_ATTENTION,
            ReactionOutput::Transform {
                new_surface: URGENT_RESPONSE.to_owned(),
            },
        ));

        // rally_beacon + fatigue → cooldown.
        // (Helldivers medic insight: rallying an exhausted agent doesn't
        //  surge them forward — it dampens the beacon so other agents
        //  can pick up. The beacon is consumed, fatigue is replaced.)
        t.add(SurfaceReaction::new(
            RALLY_BEACON,
            FATIGUE,
            ReactionOutput::Transform {
                new_surface: COOLDOWN.to_owned(),
            },
        ));

        // rally_beacon + high_attention → urgent_response.
        // (DRG laser-pointer + aggro = coordinated strike. Attention
        //  acknowledges the rally and the formation converges.)
        t.add(SurfaceReaction::new(
            RALLY_BEACON,
            HIGH_ATTENTION,
            ReactionOutput::Transform {
                new_surface: URGENT_RESPONSE.to_owned(),
            },
        ));

        // handoff_ready + role_vacancy → consensus_call.
        // (A producer finished output AND a role opened; the formation
        //  must decide the assignee via §11 consensus rather than
        //  first-come-first-served.)
        t.add(SurfaceReaction::new(
            HANDOFF_READY,
            ROLE_VACANCY,
            ReactionOutput::Transform {
                new_surface: CONSENSUS_CALL.to_owned(),
            },
        ));

        // urgent_response + cooldown → urgent_response survives,
        // cooldown consumed. Urgency overrides dampening at the moment
        // the urgency is stamped; cooldown re-applies on the next event.
        t.add(SurfaceReaction::new(
            URGENT_RESPONSE,
            COOLDOWN,
            ReactionOutput::ConsumeB { modify_a: None },
        ));

        // fatigue + fresh_input → cooldown.
        // (Tired agents defer incoming work. Input is consumed, fatigue
        //  is transformed into the dampening cooldown surface so future
        //  signals within the cooldown window get suppressed.)
        t.add(SurfaceReaction::new(
            FATIGUE,
            FRESH_INPUT,
            ReactionOutput::Transform {
                new_surface: COOLDOWN.to_owned(),
            },
        ));

        t
    }

    /// Add a reaction rule.
    pub fn add(&mut self, reaction: SurfaceReaction) {
        self.reactions.push(reaction);
    }

    /// Find the first matching reaction for two surface tags.
    pub fn lookup(&self, a: &str, b: &str) -> Option<&SurfaceReaction> {
        self.reactions.iter().find(|r| r.matches(a, b))
    }

    /// Number of registered reactions.
    pub fn len(&self) -> usize {
        self.reactions.len()
    }

    /// Whether the table has any reactions.
    pub fn is_empty(&self) -> bool {
        self.reactions.is_empty()
    }
}

impl Default for ReactionTable {
    fn default() -> Self {
        Self::cooperation_defaults()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::tags::*;
    use super::*;

    #[test]
    fn empty_table_returns_none() {
        let table = ReactionTable::new();
        assert!(table.lookup(FRESH_INPUT, HIGH_ATTENTION).is_none());
    }

    #[test]
    fn cooperation_defaults_populated() {
        let table = ReactionTable::cooperation_defaults();
        assert!(!table.is_empty());
        assert!(table.len() >= 6);
    }

    #[test]
    fn fresh_input_plus_attention_produces_urgent_response() {
        let table = ReactionTable::cooperation_defaults();
        let r = table.lookup(FRESH_INPUT, HIGH_ATTENTION).unwrap();
        match &r.output {
            ReactionOutput::Transform { new_surface } => {
                assert_eq!(new_surface, URGENT_RESPONSE);
            }
            other => panic!("expected Transform, got {other:?}"),
        }
    }

    #[test]
    fn rally_plus_fatigue_yields_cooldown() {
        let table = ReactionTable::cooperation_defaults();
        let r = table.lookup(RALLY_BEACON, FATIGUE).unwrap();
        match &r.output {
            ReactionOutput::Transform { new_surface } => {
                assert_eq!(new_surface, COOLDOWN);
            }
            other => panic!("expected Transform, got {other:?}"),
        }
    }

    #[test]
    fn lookup_order_independent() {
        let table = ReactionTable::cooperation_defaults();
        assert!(table.lookup(FRESH_INPUT, HIGH_ATTENTION).is_some());
        assert!(table.lookup(HIGH_ATTENTION, FRESH_INPUT).is_some());
    }

    #[test]
    fn custom_reaction_added() {
        let mut table = ReactionTable::new();
        table.add(SurfaceReaction::new(
            "custom_a",
            "custom_b",
            ReactionOutput::ConsumeB { modify_a: None },
        ));
        assert!(table.lookup("custom_a", "custom_b").is_some());
    }
}
