//! Shared key/value workspace — the `environment-as-medium` primitive from
//! COOPERATION.md §10.
//!
//! This is deliberately narrow: a concurrent map with a write-log, nothing
//! more. Surfaces (ambient signaling) live in `stigmergy::`; task routing
//! lives in `routing::`. The workspace is what the bot-crate blackboard
//! composes into its full façade.

pub mod shared_env;
pub mod snapshot;
pub mod trait_;
pub mod workspace;

pub use shared_env::SharedEnvironment;
pub use snapshot::WorkspaceSnapshot;
pub use trait_::Workspace;
pub use workspace::{EnvironmentWrite, InMemoryWorkspace};
