//! L-∞ formation composition — K8s-style Filter+Score admission.
//!
//! Runs once at formation creation, outside the tick loop. The composer
//! picks which agents belong to a formation given a candidate pool and a
//! spec. Filter plugins are hard predicates (pass/fail); Score plugins
//! return a `[0.0, 1.0]` soft preference; the combined utility is fed to a
//! `Picker` that selects the top-K.
//!
//! Plugins are each their own file so adding a new filter or scorer is
//! additive — a new file plus one line in `default_filters` /
//! `default_scorers`.

pub mod admission;
pub mod error;
pub mod filters;
pub mod scorers;
pub mod trait_;
pub mod types;

pub use admission::compose_formation;
pub use error::ComposeError;
pub use trait_::{AgentCandidate, FilterPlugin, FormationSpec, ScorePlugin};
pub use types::{AgentSlot, FormationComposition, RoleHint};
