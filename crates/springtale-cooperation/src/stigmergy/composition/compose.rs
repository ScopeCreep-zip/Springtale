//! compose_surfaces — the core function called by SharedEnvironment::add_surface.
//!
//! Per COOPERATION.md §10.3: "new.surfaces = compose_surfaces(&new.surfaces, &s)".
//! When a new surface enters the environment, it's checked against all existing
//! surfaces for pairwise reactions. Reactions fire once per matching pair.

use std::time::Duration;

use crate::cadence::AgentId;
use crate::stigmergy::types::{Surface, SurfaceId, SurfaceType};

use super::reaction::{surface_tag, ReactionOutput};
use super::table::ReactionTable;

/// Result of composing a new surface with existing surfaces.
#[derive(Debug)]
pub struct CompositionResult {
    /// Surfaces that survive (unmodified or modified by reactions).
    pub surviving: Vec<Surface>,
    /// New surfaces spawned by reactions.
    pub spawned: Vec<Surface>,
    /// Indices of surfaces consumed by reactions.
    pub consumed: Vec<SurfaceId>,
}

/// Compose a new surface into the existing set, applying reactions.
///
/// Per DOS2: stamp a surface → check combo table → transform/consume/spawn.
/// Per Noita: pairwise material reactions fire in the order found.
/// Per CDDA: field overlaps trigger transformations from field_type.json.
pub fn compose_surfaces(
    existing: &[Surface],
    incoming: &Surface,
    table: &ReactionTable,
    author: AgentId,
) -> CompositionResult {
    let mut surviving: Vec<Surface> = Vec::new();
    let mut spawned: Vec<Surface> = Vec::new();
    let mut consumed: Vec<SurfaceId> = Vec::new();
    let mut incoming_consumed = false;

    let incoming_tag = surface_tag(&incoming.surface_type);

    for existing_surface in existing {
        let existing_tag = surface_tag(&existing_surface.surface_type);

        if let Some(reaction) = table.lookup(existing_tag, incoming_tag) {
            match &reaction.output {
                ReactionOutput::Transform { new_surface } => {
                    consumed.push(existing_surface.id);
                    incoming_consumed = true;
                    spawned.push(Surface {
                        id: SurfaceId::new_v4(),
                        created_by: author,
                        surface_type: SurfaceType::Active {
                            remaining: Duration::from_secs(30),
                        },
                        data: serde_json::json!({
                            "origin": new_surface,
                            "from_a": existing_tag,
                            "from_b": incoming_tag,
                        }),
                        expires: None,
                        capability: None,
                    });
                }
                ReactionOutput::ConsumeA { modify_b } => {
                    consumed.push(existing_surface.id);
                    if let Some(new_type) = modify_b {
                        let mut modified_incoming = incoming.clone();
                        modified_incoming.surface_type = SurfaceType::Primed {
                            trigger: crate::cadence::ActionDescriptor {
                                kind: new_type.clone(),
                                target: None,
                                payload_hash: 0,
                            },
                        };
                        if !incoming_consumed {
                            spawned.push(modified_incoming);
                            incoming_consumed = true;
                        }
                    }
                }
                ReactionOutput::ConsumeB { modify_a } => {
                    incoming_consumed = true;
                    if let Some(new_type) = modify_a {
                        let mut modified = existing_surface.clone();
                        modified.surface_type = SurfaceType::Primed {
                            trigger: crate::cadence::ActionDescriptor {
                                kind: new_type.clone(),
                                target: None,
                                payload_hash: 0,
                            },
                        };
                        surviving.push(modified);
                    } else {
                        surviving.push(existing_surface.clone());
                    }
                }
                ReactionOutput::Spawn { new_surface } => {
                    surviving.push(existing_surface.clone());
                    spawned.push(Surface {
                        id: SurfaceId::new_v4(),
                        created_by: author,
                        surface_type: SurfaceType::Active {
                            remaining: Duration::from_secs(30),
                        },
                        data: serde_json::json!({
                            "origin": new_surface,
                            "from_a": existing_tag,
                            "from_b": incoming_tag,
                        }),
                        expires: None,
                        capability: None,
                    });
                }
                ReactionOutput::Inert => {
                    surviving.push(existing_surface.clone());
                }
            }
        } else {
            surviving.push(existing_surface.clone());
        }
    }

    if !incoming_consumed {
        surviving.push(incoming.clone());
    }

    CompositionResult {
        surviving,
        spawned,
        consumed,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::stigmergy::composition::table::ReactionTable;

    fn make_surface(tag: &str, agent: AgentId) -> Surface {
        Surface {
            id: SurfaceId::new_v4(),
            created_by: agent,
            surface_type: SurfaceType::Primed {
                trigger: crate::cadence::ActionDescriptor {
                    kind: tag.to_owned(),
                    target: None,
                    payload_hash: 0,
                },
            },
            data: serde_json::json!({}),
            expires: None,
            capability: None,
        }
    }

    #[test]
    fn no_reaction_keeps_both() {
        let table = ReactionTable::new();
        let agent = AgentId::new();
        let existing = vec![make_surface("rock", agent)];
        let incoming = make_surface("sand", agent);

        let result = compose_surfaces(&existing, &incoming, &table, agent);
        assert_eq!(result.surviving.len(), 2);
        assert!(result.spawned.is_empty());
        assert!(result.consumed.is_empty());
    }

    #[test]
    fn transform_consumes_both_spawns_new() {
        let table = ReactionTable::default_ecology();
        let agent = AgentId::new();
        let existing = vec![make_surface("water", agent)];
        let incoming = make_surface("fire", agent);

        let result = compose_surfaces(&existing, &incoming, &table, agent);
        assert_eq!(result.consumed.len(), 1);
        assert_eq!(result.spawned.len(), 1);
        let spawned_data = &result.spawned[0].data;
        assert_eq!(spawned_data["origin"], "steam");
    }

    #[test]
    fn consume_a_removes_existing() {
        let table = ReactionTable::default_ecology();
        let agent = AgentId::new();
        let existing = vec![make_surface("oil", agent)];
        let incoming = make_surface("fire", agent);

        let result = compose_surfaces(&existing, &incoming, &table, agent);
        assert_eq!(result.consumed.len(), 1);
    }

    #[test]
    fn spawn_keeps_both_adds_new() {
        let table = ReactionTable::default_ecology();
        let agent = AgentId::new();
        let existing = vec![make_surface("lava", agent)];
        let incoming = make_surface("water", agent);

        let result = compose_surfaces(&existing, &incoming, &table, agent);
        assert!(!result.spawned.is_empty());
        assert!(result.surviving.iter().any(|s| surface_tag(&s.surface_type) == "lava"));
    }

    #[test]
    fn multiple_existing_surfaces_checked_pairwise() {
        let table = ReactionTable::default_ecology();
        let agent = AgentId::new();
        let existing = vec![
            make_surface("rock", agent),
            make_surface("water", agent),
        ];
        let incoming = make_surface("fire", agent);

        let result = compose_surfaces(&existing, &incoming, &table, agent);
        // rock has no reaction with fire → survives
        // water + fire → steam (transform)
        assert!(result.surviving.iter().any(|s| surface_tag(&s.surface_type) == "rock"));
        assert_eq!(result.consumed.len(), 1); // water consumed
    }
}
