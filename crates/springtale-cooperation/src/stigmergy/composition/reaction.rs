//! Surface reactions — Noita/CDDA/DOS2 elemental combo model.
//!
//! Per COOPERATION.md §10: "water + electricity = shocked water,
//! water + fire = evaporate". Surfaces interact when overlapping.
//! Per Noita: reactions are data-table entries checked pairwise.
//! Per DOS2: surfaces apply tickable statuses, not raw damage.

use crate::stigmergy::types::SurfaceType;

/// A reaction rule between two surface types.
///
/// Per Noita's `data/materials.xml` `<Reaction>` tag: each entry
/// defines `input_cell1`, `input_cell2`, `output_cell`, with optional
/// probability. Per CDDA `field_type.json`: fields transform into
/// other fields after duration or on contact with another field.
#[derive(Debug, Clone)]
pub struct SurfaceReaction {
    pub input_a: String,
    pub input_b: String,
    pub output: ReactionOutput,
    pub probability: f32,
}

/// What happens when two surfaces react.
#[derive(Debug, Clone)]
pub enum ReactionOutput {
    /// Both inputs consumed, new surface created.
    /// DOS2: water + fire = steam cloud.
    Transform { new_surface: String },
    /// Input A consumed, B modified.
    /// DOS2: oil + fire = fire spreads, oil consumed.
    ConsumeA { modify_b: Option<String> },
    /// Input B consumed, A modified.
    ConsumeB { modify_a: Option<String> },
    /// Both persist, a third surface is spawned.
    /// Noita: lava + water = rock + steam (both inputs partially consumed).
    Spawn { new_surface: String },
    /// No reaction (explicit no-op for table completeness).
    Inert,
}

impl SurfaceReaction {
    pub fn new(input_a: &str, input_b: &str, output: ReactionOutput) -> Self {
        Self {
            input_a: input_a.to_owned(),
            input_b: input_b.to_owned(),
            output,
            probability: 1.0,
        }
    }

    pub fn with_probability(mut self, p: f32) -> Self {
        self.probability = p.clamp(0.0, 1.0);
        self
    }

    /// Check if this reaction applies to the given pair (order-independent).
    pub fn matches(&self, a: &str, b: &str) -> bool {
        (self.input_a == a && self.input_b == b) || (self.input_a == b && self.input_b == a)
    }
}

/// Classify a SurfaceType into its string tag for reaction matching.
pub fn surface_tag(st: &SurfaceType) -> &str {
    match st {
        SurfaceType::Substrate => "substrate",
        SurfaceType::Primed { trigger } => trigger.kind.as_str(),
        SurfaceType::Active { .. } => "active",
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn reaction_matches_both_orders() {
        let r = SurfaceReaction::new(
            "water",
            "fire",
            ReactionOutput::Transform {
                new_surface: "steam".to_owned(),
            },
        );
        assert!(r.matches("water", "fire"));
        assert!(r.matches("fire", "water"));
        assert!(!r.matches("water", "oil"));
    }

    #[test]
    fn probability_clamps() {
        let r = SurfaceReaction::new("a", "b", ReactionOutput::Inert).with_probability(1.5);
        assert!((r.probability - 1.0).abs() < f32::EPSILON);

        let r2 = SurfaceReaction::new("a", "b", ReactionOutput::Inert).with_probability(-0.5);
        assert!(r2.probability.abs() < f32::EPSILON);
    }

    #[test]
    fn surface_tag_extracts_trigger() {
        assert_eq!(surface_tag(&SurfaceType::Substrate), "substrate");
        assert_eq!(
            surface_tag(&SurfaceType::Primed {
                trigger: crate::cadence::ActionDescriptor {
                    kind: "electricity".to_owned(),
                    target: None,
                    payload_hash: 0,
                }
            }),
            "electricity"
        );
    }
}
