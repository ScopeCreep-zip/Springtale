//! L0 ambient signaling — stigmergy / surfaces / pheromones.
//!
//! Family 7 (Hayes-Roth blackboard, Grassé/Dorigo stigmergy). Coordination
//! state lives in the *environment*: agents deposit surfaces with TTLs, and
//! any agent whose awareness covers a primed surface can react without
//! explicit assignment. See COOPERATION.md §10 Surfaces.

pub mod awareness_match;
pub mod composition;
pub mod decay;
pub mod deposit;
pub mod trait_;
pub mod types;

pub use composition::{compose_surfaces, CompositionResult, ReactionOutput, ReactionTable, SurfaceReaction};
pub use deposit::SurfaceStore;
pub use trait_::{SurfaceDeposit, SurfaceSensor};
pub use types::{Surface, SurfaceId, SurfaceType};
