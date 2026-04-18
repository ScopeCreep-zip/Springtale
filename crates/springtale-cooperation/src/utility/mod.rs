//! Utility AI scoring — game-informed decision engine.
//!
//! Implements the scoring framework from big-brain's architecture
//! (Dave Mark's IAUS / "Building a Better Centaur" GDC talk) without
//! the Bevy ECS dependency. This is the general-purpose behavioral AI
//! engine used by agents for:
//! - Task selection (which blackboard task should I claim?)
//! - Sacrifice evaluation (§24)
//! - Recovery evaluation (§18)
//! - Support agent selection (§14/§23)
//!
//! The pipeline: Considerations → Response Curves → Composite Scorers → Picker

pub mod consideration;
pub mod evaluator;
pub mod measure;
pub mod picker;
pub mod scorer;

pub use consideration::Consideration;
pub use evaluator::ResponseCurve;
pub use measure::Measure;
pub use picker::Picker;
