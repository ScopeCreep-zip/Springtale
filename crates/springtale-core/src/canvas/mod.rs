//! Canvas/A2UI — structured content the bot pushes to the frontend.
//!
//! Per ARCHITECTURE.md: "A live UI surface that the agent can
//! programmatically push content to."

pub mod types;

pub use types::{CanvasBlock, CanvasState, CanvasUpdate, StatusState};
