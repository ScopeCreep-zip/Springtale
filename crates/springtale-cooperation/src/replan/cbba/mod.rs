//! CBBA two-phase algorithm: bundle build + consensus gossip.

pub mod bundle;
pub mod consensus;
pub mod convergence;
pub mod dmg;
pub mod orchestrator;
pub mod trait_;
pub mod types;

pub use orchestrator::{run, AgentSpec, ReplanOutcome};
pub use trait_::{BundleBuilder, ConsensusGossip};
pub use types::{Bundle, ConvergenceStatus};
