//! Reaction table — data-driven surface interaction rules.
//!
//! Per Noita: `data/materials.xml` is a flat table of `<Reaction>` entries.
//! Per CDDA: `field_type.json` defines field-to-field transformations.
//! Per DOS2: surface combos are engine-checked at stamp time.
//!
//! Our table is a Vec<SurfaceReaction> scanned linearly — formations
//! have O(10) surface types, not O(10K) like Noita's full material sim.

use super::reaction::{ReactionOutput, SurfaceReaction};

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

    /// Create a table with the default ecology-inspired reactions.
    pub fn default_ecology() -> Self {
        let mut table = Self::new();

        // DOS2-inspired: water + fire = steam
        table.add(SurfaceReaction::new(
            "water",
            "fire",
            ReactionOutput::Transform {
                new_surface: "steam".to_owned(),
            },
        ));

        // DOS2-inspired: oil + fire = explosion (fire spreads)
        table.add(SurfaceReaction::new(
            "oil",
            "fire",
            ReactionOutput::ConsumeA {
                modify_b: Some("inferno".to_owned()),
            },
        ));

        // DOS2-inspired: water + electricity = shocked_water
        table.add(SurfaceReaction::new(
            "water",
            "electricity",
            ReactionOutput::Transform {
                new_surface: "shocked_water".to_owned(),
            },
        ));

        // Noita-inspired: lava + water = rock + steam
        table.add(SurfaceReaction::new(
            "lava",
            "water",
            ReactionOutput::Spawn {
                new_surface: "rock".to_owned(),
            },
        ));

        // Springtail ecology: alarm_pheromone + food_trail = recruitment_surge
        table.add(SurfaceReaction::new(
            "alarm_pheromone",
            "food_trail",
            ReactionOutput::Transform {
                new_surface: "recruitment_surge".to_owned(),
            },
        ));

        // Springtail ecology: decay_marker + moisture = fungal_bloom
        table.add(SurfaceReaction::new(
            "decay_marker",
            "moisture",
            ReactionOutput::Spawn {
                new_surface: "fungal_bloom".to_owned(),
            },
        ));

        table
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
        Self::new()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn empty_table_returns_none() {
        let table = ReactionTable::new();
        assert!(table.lookup("water", "fire").is_none());
    }

    #[test]
    fn default_ecology_has_reactions() {
        let table = ReactionTable::default_ecology();
        assert!(!table.is_empty());
        assert!(table.len() >= 4);
    }

    #[test]
    fn lookup_finds_water_fire() {
        let table = ReactionTable::default_ecology();
        let r = table.lookup("water", "fire").unwrap();
        assert!(matches!(r.output, ReactionOutput::Transform { .. }));
    }

    #[test]
    fn lookup_order_independent() {
        let table = ReactionTable::default_ecology();
        assert!(table.lookup("fire", "water").is_some());
        assert!(table.lookup("water", "fire").is_some());
    }

    #[test]
    fn custom_reaction_added() {
        let mut table = ReactionTable::new();
        table.add(SurfaceReaction::new(
            "acid",
            "metal",
            ReactionOutput::ConsumeB { modify_a: None },
        ));
        assert!(table.lookup("acid", "metal").is_some());
    }
}
