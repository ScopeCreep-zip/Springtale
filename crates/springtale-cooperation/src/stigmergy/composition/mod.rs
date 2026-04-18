//! Surface composition — Noita/CDDA/DOS2 elemental reaction model.
//!
//! Per COOPERATION.md §10: surfaces interact pairwise via a data-driven
//! reaction table. When a new surface enters, it's checked against all
//! existing surfaces for matching reactions.

pub mod compose;
pub mod reaction;
pub mod table;

pub use compose::{compose_surfaces, CompositionResult};
pub use reaction::{ReactionOutput, SurfaceReaction};
pub use table::ReactionTable;
