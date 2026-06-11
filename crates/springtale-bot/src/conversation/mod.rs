//! Deterministic conversational task-setup — "feels like an LLM,
//! without using one."
//!
//! The user types what they want in plain words ("send me the weather
//! in Tucson every morning") and the bot understands the intent,
//! extracts what it can, asks only for what's missing, confirms, and
//! deploys the matching recipe — all with ZERO AI in the base path
//! (NoopAdapter parity). AI, when present, only augments this engine
//! (see [`augment`]); it is never required.
//!
//! Pipeline: `catalog` projects the recipe library into searchable
//! intent docs + slot gazetteers; `nlu` ranks intent and extracts
//! entities deterministically; `dialogue` runs the slot-filling state
//! machine (persisted in the session); `nlg` renders varied, human
//! replies; `deploy` is the port that materializes the chosen recipe.

pub mod augment;
pub mod catalog;
pub mod deploy;
pub mod dialogue;
pub mod engine;
pub mod error;
pub mod nlg;
pub mod nlu;

pub use deploy::{DeployError, RecipeDeployer, SharedDeployer};
pub use engine::{capability_reply, continue_active, try_start};
pub use error::ConversationError;
