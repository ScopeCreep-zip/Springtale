//! Platform verbs — the set of things chat may ask the platform to do.
//!
//! Plan 5.4. Chat gets the four orchestration verb groups plus
//! inspection, and never an assign verb (the drum rule).

pub mod registry;
pub mod run;
pub mod verb;

pub use registry::{find_verb, find_verb_by_tool_segment, platform_verbs, verb_commands};
pub use run::run_platform_verb;
pub use verb::{PlatformVerb, VerbGroup};
